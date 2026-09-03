//! Cookie 管理 Command：查看 / 清理自管 Jar 中的登录态。
//!
//! 原来 reqwest 内建 jar 是黑盒（无法查看/清理，登录态问题只能重启应用）。
//! `fox-http::cookie::ManagedJar` 保持相同的自动回放语义，本模块暴露管理面。

use serde::Serialize;

use crate::error::CommandResult;

/// 前端展示用 Cookie 条目（value 原样返回；请勿截图外传）。
#[derive(Debug, Clone, Serialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// RFC3339 到期时间；会话 Cookie 为 None。
    pub expires_at: Option<String>,
    pub secure: bool,
    pub http_only: bool,
}

/// 列出 Jar 中的 Cookie（`domain` 为空返回全部；否则子串过滤域名）。
#[tauri::command(rename_all = "camelCase")]
pub async fn cookie_list(domain: Option<String>) -> CommandResult<Vec<CookieEntry>> {
    let filter = domain.filter(|d| !d.trim().is_empty());
    Ok(fox_http::cookie::shared_jar()
        .list(filter.as_deref())
        .into_iter()
        .map(|c| CookieEntry {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            expires_at: c
                .expires_at
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
            secure: c.secure,
            http_only: c.http_only,
        })
        .collect())
}

/// 清理 Cookie（`domain` 为空=全部；否则精确域 + 子域）。返回删除条数。
#[tauri::command(rename_all = "camelCase")]
pub async fn cookie_clear(domain: Option<String>) -> CommandResult<u64> {
    let filter = domain.filter(|d| !d.trim().is_empty());
    Ok(fox_http::cookie::shared_jar().clear(filter.as_deref()) as u64)
}
