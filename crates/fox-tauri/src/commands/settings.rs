//! HTTP 设置 Command：全局代理（持久化 + 应用到 fox-http 共享客户端）。

use tauri::State;

use fox_storage::repository as repo;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// settings 表中的代理键；值为 JSON 字符串（`null` = 直连）。
const PROXY_KEY: &str = "http_proxy";

/// settings 表中的请求超时键；值为 JSON 数字（毫秒）。
const HTTP_TIMEOUT_KEY: &str = "http_timeout_ms";

/// 读取请求超时设置（毫秒）。未配置时返回 `None`，由调用方兜底默认值。
pub async fn read_http_timeout_ms(db: &sqlx::SqlitePool) -> CommandResult<Option<u64>> {
    let raw = repo::get_setting(db, HTTP_TIMEOUT_KEY).await?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str::<Option<u64>>(&json)
            .map_err(|e| CommandError::with_code("INTERNAL", format!("超时设置解析失败：{e}"))),
    }
}

/// 读取全局请求超时（毫秒；未设置时返回 None，前端展示用）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_http_timeout_ms(state: State<'_, AppState>) -> CommandResult<Option<u64>> {
    read_http_timeout_ms(&state.db).await
}

/// 设置全局请求超时（毫秒；范围 1000ms ~ 1h）。
#[tauri::command(rename_all = "camelCase")]
pub async fn set_http_timeout_ms(state: State<'_, AppState>, timeout_ms: u64) -> CommandResult<()> {
    if !(1000..=3_600_000).contains(&timeout_ms) {
        return Err(CommandError::validation("超时需在 1 秒 ~ 1 小时之间"));
    }
    let json = serde_json::to_string(&Some(timeout_ms))
        .map_err(|e| CommandError::with_code("INTERNAL", format!("序列化失败：{e}")))?;
    repo::set_setting(&state.db, HTTP_TIMEOUT_KEY, &json).await?;
    Ok(())
}

/// 读取全局代理地址（None = 直连）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_http_proxy(state: State<'_, AppState>) -> CommandResult<Option<String>> {
    let raw = repo::get_setting(&state.db, PROXY_KEY)
        .await
        .map_err(CommandError::from)?;
    match raw {
        None => Ok(None),
        Some(json) => serde_json::from_str::<Option<String>>(&json)
            .map_err(|e| CommandError::with_code("INTERNAL", format!("代理设置解析失败：{e}"))),
    }
}

/// 设置全局代理（`http://host:port` / `socks5://host:port`；None = 直连）。
///
/// 持久化到 settings 并立即应用到共享 HTTP 客户端；应用启动时
/// （[`crate::state`] 初始化后）通过 [`apply_saved_proxy`] 恢复。
#[tauri::command(rename_all = "camelCase")]
pub async fn set_http_proxy(
    state: State<'_, AppState>,
    proxy: Option<String>,
) -> CommandResult<()> {
    let trimmed = proxy
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    if let Some(p) = &trimmed {
        // 提前校验格式，避免坏地址在每次发请求时才报错
        fox_http::client::validate_proxy(p)
            .map_err(|e| CommandError::validation(e.user_message()))?;
    }
    fox_http::client::set_proxy(trimmed.as_deref())
        .map_err(|e| CommandError::validation(e.user_message()))?;
    let json = serde_json::to_string(&trimmed)
        .map_err(|e| CommandError::with_code("INTERNAL", format!("序列化失败：{e}")))?;
    repo::set_setting(&state.db, PROXY_KEY, &json)
        .await
        .map_err(CommandError::from)?;
    Ok(())
}

/// 代理连通性测试结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    pub ok: bool,
    pub status: u16,
    pub duration_ms: f64,
    pub message: String,
}

/// 测试全局代理连通性：经当前共享客户端（含已设代理）向目标 URL 发一次请求。
///
/// 复用 `execute_request` 同款客户端，因此若代理已设置则请求会走代理；未设置代理
/// 时即直连探测（用于校验目标可达性）。超时 8 秒，不落历史。
#[tauri::command(rename_all = "camelCase")]
pub async fn test_http_proxy(target: Option<String>) -> CommandResult<ProxyTestResult> {
    use fox_core::model::{HttpMethod, RequestSpec};
    let url = target
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "https://www.gstatic.com/generate_204".to_string());
    let spec = RequestSpec::default();
    let started = std::time::Instant::now();
    match fox_http::client::send_request(HttpMethod::GET, &url, &spec, Some(8_000)).await {
        Ok(resp) => Ok(ProxyTestResult {
            ok: true,
            status: resp.status,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            message: format!(
                "连通成功（HTTP {}，{:.0}ms）",
                resp.status, resp.duration_ms
            ),
        }),
        Err(e) => Ok(ProxyTestResult {
            ok: false,
            status: 0,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            message: format!("连通失败：{}", e.user_message()),
        }),
    }
}

/// 启动时恢复持久化的代理设置（设置加载失败时静默保持直连）。
pub async fn apply_saved_proxy(db: &sqlx::SqlitePool) {
    let raw = match repo::get_setting(db, PROXY_KEY).await {
        Ok(Some(json)) => json,
        _ => return,
    };
    if let Ok(Some(proxy)) = serde_json::from_str::<Option<String>>(&raw) {
        if !proxy.is_empty() {
            let _ = fox_http::client::set_proxy(Some(proxy.as_str()));
        }
    }
}
