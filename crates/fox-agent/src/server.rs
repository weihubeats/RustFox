//! Agent 控制面 HTTP 服务：axum 路由 + Bearer 令牌鉴权。
//!
//! 端口从 4110 起探测（避开 Mock 的 4010~4029 段），只绑定回环地址。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use fox_core::model::{Endpoint, Project};
use fox_storage::repository as repo;
use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::import::endpoint_from_curl;

/// 默认端口（4110~4129 探测；与 Mock 的 4010~4029 错开）。
pub const DEFAULT_AGENT_PORT: u16 = 4110;
/// 端口被占用时最多尝试的次数。
pub const MAX_PORT_TRIES: u16 = 20;

/// 导入成功后广播给 UI 层的事件。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AgentEvent {
    /// 一个接口由 Agent 导入成功。
    EndpointImported {
        endpoint_id: uuid::Uuid,
        project_id: uuid::Uuid,
        name: String,
    },
}

/// 服务共享状态：连接池 + 鉴权令牌 + 事件广播。
#[derive(Clone)]
pub struct AgentState {
    pub db: SqlitePool,
    token: Arc<String>,
    events: broadcast::Sender<AgentEvent>,
}

impl AgentState {
    pub fn new(db: SqlitePool, token: impl Into<String>) -> Self {
        let (events, _) = broadcast::channel(64);
        AgentState {
            db,
            token: Arc::new(token.into()),
            events,
        }
    }

    /// 订阅事件（需在 `start` 前调用，避免漏掉最早的导入）。
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }
}

// ---------- 请求 / 响应体 ----------

/// POST `/agent/curl` 请求体。字段同时接受 camelCase 与 snake_case。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCurlRequest {
    /// 完整 cURL 命令字符串。
    pub command: String,
    /// 目标项目 id；缺省时：仅一个项目 → 自动选中，零个项目 → 创建「Agent 导入」，
    /// 多个项目 → 返回错误并列出候选。
    #[serde(alias = "project_id")]
    pub project_id: Option<uuid::Uuid>,
    /// 接口名称；缺省从 URL 路径推导。
    #[serde(default)]
    pub name: Option<String>,
    /// 归属文件夹 id（可选）。
    #[serde(alias = "folder_id")]
    pub folder_id: Option<uuid::Uuid>,
}

/// POST `/agent/curl` 响应体。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCurlResult {
    pub endpoint_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: String,
    pub method: String,
    pub path: String,
    /// 项目变量 `base_url` 的最终值（可能因本次导入而新建）。
    pub base_url: String,
    /// 非致命提示（如项目已有不同 base_url），Agent 应转达用户。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// 统一错误响应体 `{ code, message }`。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ApiErrorBody {
    code: String,
    message: String,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiErrorBody>)>;

fn api_error(err: fox_core::AppError) -> (StatusCode, Json<ApiErrorBody>) {
    let (status, code) = match &err {
        fox_core::AppError::Validation(_) | fox_core::AppError::Json(_) => {
            (StatusCode::BAD_REQUEST, "VALIDATION")
        }
        fox_core::AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"),
    };
    (
        status,
        Json(ApiErrorBody {
            code: code.to_string(),
            message: err.user_message(),
        }),
    )
}

// ---------- 中间件 ----------

async fn auth(State(state): State<AgentState>, req: Request, next: Next) -> Response {
    let provided = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| {
            req.headers()
                .get("x-agent-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        });
    // 不泄露 token 内容，只记录是否缺失。
    if provided.as_deref() != Some(state.token.as_str()) {
        tracing::warn!("[agent] 未授权请求：{} {}", req.method(), req.uri().path());
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiErrorBody {
                code: "UNAUTHORIZED".into(),
                message: "缺少或错误的令牌（Authorization: Bearer <token>）".into(),
            }),
        )
            .into_response();
    }
    next.run(req).await
}

// ---------- Handlers ----------

async fn health() -> &'static str {
    "ok"
}

async fn list_projects_handler(State(state): State<AgentState>) -> ApiResult<Vec<Project>> {
    repo::list_projects(&state.db)
        .await
        .map(Json)
        .map_err(api_error)
}

async fn list_endpoints_handler(
    State(state): State<AgentState>,
    AxumPath(project_id): AxumPath<uuid::Uuid>,
) -> ApiResult<Vec<Endpoint>> {
    repo::list_endpoints(&state.db, project_id)
        .await
        .map(Json)
        .map_err(api_error)
}

/// 解析目标项目：显式 id 校验存在性；缺省时按「唯一项目自动选中 /
/// 零项目自动创建 / 多项目报错列出候选」的规则收敛。
async fn resolve_project(
    db: &SqlitePool,
    project_id: Option<uuid::Uuid>,
) -> Result<Project, fox_core::AppError> {
    if let Some(id) = project_id {
        return repo::get_project(db, id).await;
    }
    let projects = repo::list_projects(db).await?;
    match projects.len() {
        0 => {
            let now = chrono::Utc::now();
            let project = Project {
                id: uuid::Uuid::new_v4(),
                name: "Agent 导入".into(),
                description: "由 AI Agent 自动创建".into(),
                variables: Default::default(),
                created_at: now,
                updated_at: now,
            };
            repo::save_project(db, &project).await?;
            Ok(project)
        }
        1 => Ok(projects.into_iter().next().unwrap()),
        n => Err(fox_core::AppError::Validation(format!(
            "存在 {n} 个项目且未指定 projectId，请先 GET /agent/projects 后在请求中指定。候选：{}",
            projects
                .iter()
                .take(5)
                .map(|p| format!("{}({})", p.name, p.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// 维护项目变量 `base_url`：
/// - 缺失 → 写入本次导入的 origin（返回新值）；
/// - 已有且一致 → 原样返回；
/// - 已有但不一致 → 不覆盖用户配置，返回 warning 由 Agent 转达。
async fn ensure_base_url(
    db: &SqlitePool,
    project: &Project,
    origin: &str,
) -> Result<(String, Option<String>), fox_core::AppError> {
    match project.variables.get("base_url") {
        Some(existing) if existing == origin => Ok((origin.to_string(), None)),
        Some(existing) => Ok((
            existing.clone(),
            Some(format!(
                "项目「{}」的 base_url 为 {existing}，与导入 URL 的来源 {origin} 不一致；接口已按相对路径保存，发送前请确认 base_url。",
                project.name
            )),
        )),
        None => {
            let mut updated = project.clone();
            updated.variables.insert("base_url".into(), origin.to_string());
            updated.updated_at = chrono::Utc::now();
            repo::save_project(db, &updated).await?;
            Ok((origin.to_string(), None))
        }
    }
}

async fn import_curl_handler(
    State(state): State<AgentState>,
    Json(req): Json<ImportCurlRequest>,
) -> ApiResult<ImportCurlResult> {
    let parsed = fox_core::parse_curl(&req.command).map_err(api_error)?;
    let project = resolve_project(&state.db, req.project_id)
        .await
        .map_err(api_error)?;
    let (mut endpoint, origin) =
        endpoint_from_curl(parsed, project.id, req.folder_id, req.name).map_err(api_error)?;
    let (base_url, warning) = ensure_base_url(&state.db, &project, &origin)
        .await
        .map_err(api_error)?;

    repo::save_endpoint(&state.db, &endpoint)
        .await
        .map_err(api_error)?;

    let _ = state.events.send(AgentEvent::EndpointImported {
        endpoint_id: endpoint.id,
        project_id: endpoint.project_id,
        name: endpoint.name.clone(),
    });
    tracing::info!(
        "[agent] cURL 已导入为接口 {} {}（项目 {}）",
        endpoint.method.as_str(),
        endpoint.path,
        project.name
    );

    Ok(Json(ImportCurlResult {
        endpoint_id: endpoint.id,
        project_id: endpoint.project_id,
        name: std::mem::take(&mut endpoint.name),
        method: endpoint.method.as_str().to_string(),
        path: std::mem::take(&mut endpoint.path),
        base_url,
        warning,
    }))
}

/// 构建路由（pub 供 rustfox-mcp 集成测试复用）。
pub fn build_router(state: AgentState) -> Router {
    Router::new()
        .route("/agent/health", get(health))
        .route("/agent/projects", get(list_projects_handler))
        .route("/agent/endpoints/:project_id", get(list_endpoints_handler))
        .route("/agent/curl", post(import_curl_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

/// 正在运行的 Agent 服务句柄。
pub struct AgentServer {
    pub port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl AgentServer {
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// 停止服务（幂等）。
    pub async fn stop(mut self) {
        let _ = self.shutdown.send(());
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

/// 启动服务：端口从 4110 起依次尝试。
pub async fn start(state: AgentState) -> Result<AgentServer, fox_core::AppError> {
    for port in DEFAULT_AGENT_PORT..DEFAULT_AGENT_PORT + MAX_PORT_TRIES {
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let Ok(listener) = tokio::net::TcpListener::bind(addr).await else {
            continue;
        };
        let router = build_router(state.clone());
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        tracing::info!("[agent] Agent 控制面已启动：http://127.0.0.1:{port}");
        return Ok(AgentServer {
            port,
            shutdown: tx,
            handle: Some(handle),
        });
    }
    Err(fox_core::AppError::Mock(format!(
        "端口 {}~{} 均被占用，无法启动 Agent 控制面",
        DEFAULT_AGENT_PORT,
        DEFAULT_AGENT_PORT + MAX_PORT_TRIES - 1
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::load_or_create_token;

    async fn spawn_test_server() -> (String, String, SqlitePool, AgentState) {
        let dir = std::env::temp_dir().join(format!("rustfox-agent-srv-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test.db");
        let db = fox_storage::db::init_db(&db_path).await.expect("建库");
        let token = load_or_create_token(&dir).expect("token");
        let state = AgentState::new(db.clone(), token.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = build_router(state.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let url = format!("http://{addr}");
        (url, token, db, state)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    #[tokio::test]
    async fn rejects_missing_and_wrong_token() {
        let (url, token, _db, _state) = spawn_test_server().await;
        // 无 token → 401
        let res = client()
            .get(format!("{url}/agent/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // 错误 token → 401
        let res = client()
            .get(format!("{url}/agent/health"))
            .header("Authorization", "Bearer wrong")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // 正确 token → 200
        let res = client()
            .get(format!("{url}/agent/health"))
            .header("Authorization", &format!("Bearer {token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // X-Agent-Token 头同样可用
        let res = client()
            .get(format!("{url}/agent/health"))
            .header("X-Agent-Token", &token)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn import_curl_creates_project_and_endpoint() {
        let (url, token, db, state) = spawn_test_server().await;
        let mut rx = state.subscribe();

        let command = r#"curl -X POST -H "Content-Type: application/json" -d '{"sku":"A1"}' https://api.example.com/orders?channel=app"#;
        let res: ImportCurlResult = client()
            .post(format!("{url}/agent/curl"))
            .header("Authorization", &format!("Bearer {token}"))
            .json(&serde_json::json!({ "command": command }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(res.method, "POST");
        assert_eq!(res.path, "/orders");
        assert_eq!(res.base_url, "https://api.example.com");
        assert!(res.warning.is_none());

        // 事件已广播
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, AgentEvent::EndpointImported { .. }));

        // 落库可查：自动创建了「Agent 导入」项目 + base_url 变量
        let projects = repo::list_projects(&db).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].variables.get("base_url").map(String::as_str),
            Some("https://api.example.com")
        );
        let endpoints = repo::list_endpoints(&db, projects[0].id).await.unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].path, "/orders");
        assert_eq!(endpoints[0].request.params[0].key, "channel");

        // 列表端点可用
        let list: Vec<serde_json::Value> = client()
            .get(format!("{url}/agent/endpoints/{}", projects[0].id))
            .header("X-Agent-Token", &token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(list.len(), 1);

        db.close().await;
    }

    #[tokio::test]
    async fn ambiguous_project_without_id_returns_candidates() {
        let (url, token, db, _state) = spawn_test_server().await;
        for name in ["A", "B"] {
            let now = chrono::Utc::now();
            repo::save_project(
                &db,
                &Project {
                    id: uuid::Uuid::new_v4(),
                    name: name.into(),
                    description: String::new(),
                    variables: Default::default(),
                    created_at: now,
                    updated_at: now,
                },
            )
            .await
            .unwrap();
        }
        let res = client()
            .post(format!("{url}/agent/curl"))
            .header("Authorization", &format!("Bearer {token}"))
            .json(&serde_json::json!({ "command": "curl https://x.com/ping" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body: ApiErrorBody = res.json().await.unwrap();
        assert_eq!(body.code, "VALIDATION");
        assert!(body.message.contains("GET /agent/projects"));
        db.close().await;
    }

    #[tokio::test]
    async fn base_url_conflict_yields_warning_not_overwrite() {
        let (url, token, db, _state) = spawn_test_server().await;
        let now = chrono::Utc::now();
        let project = Project {
            id: uuid::Uuid::new_v4(),
            name: "已有项目".into(),
            description: String::new(),
            variables: [(
                "base_url".to_string(),
                "https://prod.example.com".to_string(),
            )]
            .into_iter()
            .collect(),
            created_at: now,
            updated_at: now,
        };
        repo::save_project(&db, &project).await.unwrap();

        let res: ImportCurlResult = client()
            .post(format!("{url}/agent/curl"))
            .header("Authorization", &format!("Bearer {token}"))
            .json(&serde_json::json!({
                "command": "curl https://other.example.com/items",
                "projectId": project.id
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(
            res.base_url, "https://prod.example.com",
            "不得覆盖既有 base_url"
        );
        let warn = res.warning.expect("应包含不一致警告");
        assert!(warn.contains("https://other.example.com"));

        // 用户数据未被篡改
        let reloaded = repo::get_project(&db, project.id).await.unwrap();
        assert_eq!(
            reloaded.variables.get("base_url").map(String::as_str),
            Some("https://prod.example.com")
        );
        db.close().await;
    }

    #[tokio::test]
    async fn malformed_command_is_400() {
        let (url, token, _db, _state) = spawn_test_server().await;
        let res = client()
            .post(format!("{url}/agent/curl"))
            .header("Authorization", &format!("Bearer {token}"))
            .json(&serde_json::json!({ "command": "curl -X GET -H 'a: 1'" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
