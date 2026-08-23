//! 项目 Command：增删改查 + 激活上下文切换。

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use fox_core::model::Project;
use fox_storage::repository as repo;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// 列出全部项目（按拖拽排序 sort_order 排列）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_projects(state: State<'_, AppState>) -> CommandResult<Vec<Project>> {
    repo::list_projects(&state.db).await.map_err(Into::into)
}

/// 项目仪表板统计（单条 IPC 替代逐项目 list_endpoints 的 N+1 加载）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStat {
    pub project_id: Uuid,
    pub endpoint_count: i64,
    pub latest_method: Option<String>,
    pub latest_path: Option<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_project_stats(
    state: State<'_, AppState>,
) -> CommandResult<Vec<ProjectStat>> {
    let stats = repo::list_endpoint_stats(&state.db).await?;
    Ok(stats
        .into_iter()
        .map(|s| ProjectStat {
            project_id: Uuid::parse_str(&s.project_id).unwrap_or_else(|_| Uuid::nil()),
            endpoint_count: s.endpoint_count,
            latest_method: s.latest_method,
            latest_path: s.latest_path,
        })
        .collect())
}

/// 拖拽排序持久化：按前端拖拽后的 id 顺序（事务）批量更新 sort_order。
#[tauri::command(rename_all = "camelCase")]
pub async fn update_projects_order(
    state: State<'_, AppState>,
    project_ids: Vec<Uuid>,
) -> CommandResult<()> {
    repo::update_projects_order(&state.db, &project_ids).await?;
    Ok(())
}

/// 创建或覆盖保存项目。
///
/// 参数校验示例：项目名称必填。校验失败返回 `{ code: "VALIDATION", message }`。
#[tauri::command(rename_all = "camelCase")]
pub async fn save_project(state: State<'_, AppState>, project: Project) -> CommandResult<Project> {
    if project.name.trim().is_empty() {
        return Err(CommandError::validation("项目名称不能为空"));
    }
    repo::save_project(&state.db, &project).await?;
    Ok(project)
}

/// 删除项目（同时清理激活上下文缓存）。
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_project(state: State<'_, AppState>, project_id: Uuid) -> CommandResult<()> {
    repo::delete_project(&state.db, project_id).await?;
    let mut active = state.active.write().await;
    if active.project_id == Some(project_id) {
        active.project_id = None;
        active.project = None;
        active.environment_id = None;
        active.environment = None;
        drop(active);
        repo::set_setting(&state.db, "active_project_id", "null").await?;
        repo::set_setting(&state.db, "active_environment_id", "null").await?;
    }
    Ok(())
}

/// 切换激活项目（`null` 表示清空）。返回切换后的项目缓存。
#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_project(
    state: State<'_, AppState>,
    project_id: Option<Uuid>,
) -> CommandResult<Option<Project>> {
    state.set_active_project(project_id).await?;
    state.active_project().await
}

/// 读取当前激活项目。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_active_project(state: State<'_, AppState>) -> CommandResult<Option<Project>> {
    state.active_project().await
}
