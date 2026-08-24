//! 控制面 HTTP 客户端：`rustfox-mcp` 等外部工具的复用层。
//!
//! - [`ControlClient::discover_default`]：从 `{data_dir}/agent-token` 读令牌，
//!   探测 `127.0.0.1:4110~4129` 的 `/agent/health`，命中即建立会话；
//! - 全部方法对应服务端同名端点；非 2xx 统一转 [`AppError`]（携带 code + message）。

use std::time::Duration;

use fox_core::error::AppError;
use fox_core::model::{Endpoint, Project};
use reqwest::Client;
use uuid::Uuid;

use crate::server::{ImportCurlResult, MAX_PORT_TRIES};

/// 默认起始端口（与服务端一致）。
pub const DEFAULT_AGENT_PORT: u16 = crate::server::DEFAULT_AGENT_PORT;

/// 探测超时（端口未监听时连接立即失败，超时只兜底极端情况）。
const PROBE_TIMEOUT: Duration = Duration::from_millis(800);

/// Agent 控制面客户端（异步）。
#[derive(Clone)]
pub struct ControlClient {
    http: Client,
    base: String,
    token: String,
}

impl ControlClient {
    /// 指定地址与令牌构造。
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
        ControlClient {
            http: Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest Client 构建失败"),
            base: base.into().trim_end_matches('/').to_string(),
            token: token.into(),
        }
    }

    /// 自动发现本机运行中的控制面：读默认令牌文件并扫描端口段。
    pub async fn discover_default() -> Result<Self, AppError> {
        let token = crate::load_or_create_token(&default_data_dir()).map_err(AppError::Io)?;
        for port in DEFAULT_AGENT_PORT..DEFAULT_AGENT_PORT + MAX_PORT_TRIES {
            let candidate = Self::new(format!("http://127.0.0.1:{port}"), &token);
            if candidate.health().await.is_ok() {
                return Ok(candidate);
            }
        }
        Err(AppError::Connection(
            "未发现运行中的 RustFox Agent 控制面（127.0.0.1:4110~4129）。请先启动 RustFox 桌面应用。"
                .into(),
        ))
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// 控制面监听地址（供 `agent_info` 工具回显）。
    pub fn base(&self) -> &str {
        &self.base
    }

    /// 存活探针。
    pub async fn health(&self) -> Result<(), AppError> {
        let res = self
            .http
            .get(format!("{}/agent/health", self.base))
            .timeout(PROBE_TIMEOUT)
            .header("Authorization", self.auth())
            .send()
            .await
            .map_err(AppError::from_reqwest)?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(AppError::Connection(format!(
                "/agent/health 返回 {}",
                res.status()
            )))
        }
    }

    /// 导入 cURL 为接口。
    #[allow(clippy::too_many_arguments)]
    pub async fn save_curl(
        &self,
        command: &str,
        name: Option<&str>,
        project_id: Option<Uuid>,
        folder_id: Option<Uuid>,
    ) -> Result<ImportCurlResult, AppError> {
        let mut body = serde_json::json!({ "command": command });
        if let Some(n) = name {
            body["name"] = serde_json::Value::String(n.to_string());
        }
        if let Some(p) = project_id {
            body["projectId"] = serde_json::Value::String(p.to_string());
        }
        if let Some(f) = folder_id {
            body["folderId"] = serde_json::Value::String(f.to_string());
        }
        self.post_json("/agent/curl", body).await
    }

    /// 项目列表。
    pub async fn list_projects(&self) -> Result<Vec<Project>, AppError> {
        self.get_json("/agent/projects").await
    }

    /// 项目下的接口列表。
    pub async fn list_endpoints(&self, project_id: Uuid) -> Result<Vec<Endpoint>, AppError> {
        self.get_json(&format!("/agent/endpoints/{project_id}"))
            .await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, AppError> {
        let res = self
            .http
            .get(format!("{}{path}", self.base))
            .header("Authorization", self.auth())
            .send()
            .await
            .map_err(AppError::from_reqwest)?;
        decode(res).await
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, AppError> {
        let res = self
            .http
            .post(format!("{}{path}", self.base))
            .header("Authorization", self.auth())
            .json(&body)
            .send()
            .await
            .map_err(AppError::from_reqwest)?;
        decode(res).await
    }
}

/// 非 2xx → 解析 `{code,message}` 转语义化错误；2xx → 反序列化目标类型。
async fn decode<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> Result<T, AppError> {
    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        // 服务端错误体形如 {code, message}
        if let Ok(body) = serde_json::from_str::<serde_json::Value>(&text) {
            let code = body["code"].as_str().unwrap_or("INTERNAL");
            let message = body["message"].as_str().unwrap_or(text.trim());
            return Err(match code {
                "VALIDATION" => AppError::Validation(message.to_string()),
                "NOT_FOUND" => AppError::NotFound(message.to_string()),
                _ => AppError::Connection(format!("[{code}] {message}")),
            });
        }
        return Err(AppError::Connection(format!("请求失败：HTTP {status}")));
    }
    res.json::<T>()
        .await
        .map_err(|e| AppError::Validation(format!("响应解析失败：{e}")))
}

/// 默认数据目录（与 fox-storage 一致：debug 构建 → RustFox-dev）。
pub fn default_data_dir() -> std::path::PathBuf {
    fox_storage::db::data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{build_router, AgentState};
    use fox_storage::db::init_db;

    async fn spawn_server() -> (String, String) {
        let dir = std::env::temp_dir().join(format!("rustfox-agent-cli-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = init_db(&dir.join("t.db")).await.unwrap();
        let token = "test-token".to_string();
        let state = AgentState::new(db.clone(), token.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, build_router(state)).await });
        (format!("http://{addr}"), token)
    }

    #[tokio::test]
    async fn client_roundtrip_save_and_list() {
        let (url, token) = spawn_server().await;
        let client = ControlClient::new(url, token);

        client.health().await.expect("探活");
        let saved = client
            .save_curl("curl https://api.example.com/ping?x=1", None, None, None)
            .await
            .expect("导入");
        assert_eq!(saved.path, "/ping");
        assert_eq!(saved.method, "GET");

        let projects = client.list_projects().await.expect("项目列表");
        assert_eq!(projects.len(), 1);
        let endpoints = client
            .list_endpoints(projects[0].id)
            .await
            .expect("接口列表");
        assert_eq!(endpoints.len(), 1);

        // 未知项目 → 服务端返回空列表（list 端点不校验项目存在性）
        let empty = client.list_endpoints(Uuid::new_v4()).await.expect("空列表");
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn wrong_token_surfaces_unauthorized() {
        let (url, _token) = spawn_server().await;
        let client = ControlClient::new(url, "bad");
        let err = client.health().await.expect_err("应 401");
        assert!(matches!(err, AppError::Connection(_)));
    }
}
