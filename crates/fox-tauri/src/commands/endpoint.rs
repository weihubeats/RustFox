//! 接口 Command：列表 / 详情 / 保存（含参数校验）/ 删除 / 复制。

use tauri::State;
use uuid::Uuid;

use fox_core::model::Endpoint;
use fox_storage::repository as repo;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// 列出项目下的全部接口。
#[tauri::command(rename_all = "camelCase")]
pub async fn list_endpoints(
    state: State<'_, AppState>,
    project_id: Uuid,
) -> CommandResult<Vec<Endpoint>> {
    repo::list_endpoints(&state.db, project_id)
        .await
        .map_err(Into::into)
}

/// 接口详情。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_endpoint(
    state: State<'_, AppState>,
    endpoint_id: Uuid,
) -> CommandResult<Endpoint> {
    repo::get_endpoint(&state.db, endpoint_id)
        .await
        .map_err(Into::into)
}

/// 保存接口（upsert）。
///
/// 参数校验：名称必填、路径必填且为 `/相对路径` 或绝对 URL。
///
/// 绝对 URL 分支：环境激活时 cURL 导入把 origin+path 整条存入 `path`
/// （见前端 `openCurlDraft`），发送走绝对地址直达导入主机——后端必须放行，
/// 口径与前端 `saveActiveDraft` 的 `/…` 或 `https?://…` 校验一致。
fn validate_endpoint_shape(name: &str, path: &str) -> CommandResult<()> {
    if name.trim().is_empty() {
        return Err(CommandError::validation("接口名称不能为空"));
    }
    if path.trim().is_empty() {
        return Err(CommandError::validation("接口路径不能为空"));
    }
    let p = path.trim();
    let lower = p.to_ascii_lowercase();
    let absolute = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ws://")
        || lower.starts_with("wss://");
    if !p.starts_with('/') && !absolute {
        return Err(CommandError::validation(
            "接口路径必须以 / 开头或为完整 URL",
        ));
    }
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_endpoint(
    state: State<'_, AppState>,
    endpoint: Endpoint,
) -> CommandResult<Endpoint> {
    validate_endpoint_shape(&endpoint.name, &endpoint.path)?;
    repo::save_endpoint(&state.db, &endpoint).await?;
    repo::touch_project(&state.db, &endpoint.project_id).await?;
    Ok(endpoint)
}

/// 删除接口。
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_endpoint(state: State<'_, AppState>, endpoint_id: Uuid) -> CommandResult<()> {
    repo::delete_endpoint(&state.db, endpoint_id)
        .await
        .map_err(Into::into)
}

/// 复制接口（返回新接口）。
#[tauri::command(rename_all = "camelCase")]
pub async fn duplicate_endpoint(
    state: State<'_, AppState>,
    endpoint_id: Uuid,
) -> CommandResult<Endpoint> {
    repo::duplicate_endpoint(&state.db, endpoint_id)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 路径校验口径：`/相对路径` 与绝对 URL（cURL 导入环境激活分支）放行，其余拒绝。
    #[test]
    fn endpoint_path_validation_accepts_relative_and_absolute() {
        // 相对路径
        assert!(validate_endpoint_shape("A", "/users").is_ok());
        // 绝对 URL（环境激活时 cURL 导入存整条 URL——回归：曾被后端拒绝无法保存）
        assert!(validate_endpoint_shape("A", "https://api.example.com/users").is_ok());
        assert!(validate_endpoint_shape("A", "HTTP://api.example.com/users").is_ok());
        assert!(validate_endpoint_shape("A", "wss://ws.example.com/sock").is_ok());
        // 拒绝：空名 / 空路径 / 既非斜杠也非绝对 URL
        assert!(validate_endpoint_shape("  ", "/x").is_err());
        assert!(validate_endpoint_shape("A", "   ").is_err());
        assert!(validate_endpoint_shape("A", "users").is_err());
    }
}
