//! 环境 Command：列表 / 保存 / 激活切换。

use tauri::State;
use uuid::Uuid;

use fox_core::model::Environment;
use fox_storage::repository as repo;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// 列出全部环境（全局维度，跨项目共享；模块已按当前项目自动同步）。
#[tauri::command(rename_all = "camelCase")]
pub async fn list_environments(state: State<'_, AppState>) -> CommandResult<Vec<Environment>> {
    repo::list_environments(&state.db)
        .await
        .map_err(Into::into)
}

/// 保存环境（upsert）。名称必填。返回同步项目模块后的完整环境。
#[tauri::command(rename_all = "camelCase")]
pub async fn save_environment(
    state: State<'_, AppState>,
    environment: Environment,
) -> CommandResult<Environment> {
    if environment.name.trim().is_empty() {
        return Err(CommandError::validation("环境名称不能为空"));
    }
    repo::save_environment(&state.db, &environment).await.map_err(Into::into)
}

/// 切换激活环境（`null` 表示不使用环境变量）。返回切换后的环境缓存。
#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_environment(
    state: State<'_, AppState>,
    environment_id: Option<Uuid>,
) -> CommandResult<Option<Environment>> {
    state.set_active_environment(environment_id).await?;
    state.active_environment().await
}

/// 读取当前激活环境。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_active_environment(
    state: State<'_, AppState>,
) -> CommandResult<Option<Environment>> {
    state.active_environment().await
}

/// 删除环境；若删除的是当前激活环境，则同时清空激活状态。
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_environment(
    state: State<'_, AppState>,
    environment_id: Uuid,
) -> CommandResult<()> {
    repo::delete_environment(&state.db, environment_id).await?;
    let mut active = state.active.write().await;
    if active.environment_id == Some(environment_id) {
        active.environment_id = None;
        active.environment = None;
    }
    Ok(())
}

/// 读取全局变量（跨项目共享，优先级最低的兜底变量表）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_global_variables(
    state: State<'_, AppState>,
) -> CommandResult<Vec<fox_core::model::EnvironmentVariable>> {
    repo::get_global_variables(&state.db)
        .await
        .map_err(Into::into)
}

/// 保存全局变量（整体覆盖写）。
#[tauri::command(rename_all = "camelCase")]
pub async fn save_global_variables(
    state: State<'_, AppState>,
    variables: Vec<fox_core::model::EnvironmentVariable>,
) -> CommandResult<()> {
    repo::save_global_variables(&state.db, &variables)
        .await
        .map_err(Into::into)
}

/// 读取全局参数（每个请求自动注入的 query / header）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_global_params(
    state: State<'_, AppState>,
) -> CommandResult<Vec<fox_core::model::GlobalParam>> {
    repo::get_global_params(&state.db).await.map_err(Into::into)
}

/// 保存全局参数（整体覆盖写）。
#[tauri::command(rename_all = "camelCase")]
pub async fn save_global_params(
    state: State<'_, AppState>,
    params: Vec<fox_core::model::GlobalParam>,
) -> CommandResult<()> {
    repo::save_global_params(&state.db, &params)
        .await
        .map_err(Into::into)
}
