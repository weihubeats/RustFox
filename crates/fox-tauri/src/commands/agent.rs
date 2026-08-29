//! Agent 控制面 Command：启动（幂等，随应用自动拉起）/ 停止 / 查询状态。
//!
//! 服务监听 `127.0.0.1`，Bearer 令牌存于 `{data_dir}/agent-token`；
//! 导入事件经 broadcast 转发为前端事件 `fox:agent-event`。

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use fox_agent::server::AgentState;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// Agent 服务状态信息（字段 snake_case，与 IPC 响应命名惯例一致）。
#[derive(Debug, Serialize)]
pub struct AgentStatusInfo {
    pub running: bool,
    /// 监听地址（未运行时为 `None`）。
    pub address: Option<String>,
    /// 令牌文件路径（Agent 配置时读取）。
    pub token_path: String,
}

/// 确保 Agent 控制面已启动（幂等）。返回监听地址。
///
/// 启动前先订阅事件通道，避免最早的导入事件丢失；事件转发任务在
/// channel 关闭（服务停止）后自然退出。
pub async fn ensure_started(app: &tauri::AppHandle) -> CommandResult<String> {
    let state: State<'_, AppState> = app.state();
    let mut guard = state.agent.write().await;
    if let Some(server) = guard.as_ref() {
        return Ok(server.address());
    }

    let token = fox_agent::load_or_create_token(&fox_storage::db::data_dir())
        .map_err(|e| CommandError::with_code("IO", format!("Agent 令牌文件创建失败：{e}")))?;
    let agent_state = AgentState::new(state.db.clone(), token);
    let mut events = agent_state.subscribe();

    let server = fox_agent::start(agent_state).await?;
    let address = server.address();
    *guard = Some(server);
    drop(guard);

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let _ = handle.emit("fox:agent-event", &event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("[agent] 事件转发滞后，丢弃 {n} 条");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok(address)
}

/// 启动 Agent 控制面（幂等，返回监听地址）。
#[tauri::command(rename_all = "camelCase")]
pub async fn agent_start(app: tauri::AppHandle) -> CommandResult<String> {
    ensure_started(&app).await
}

/// 停止 Agent 控制面（幂等）。
#[tauri::command(rename_all = "camelCase")]
pub async fn agent_stop(state: State<'_, AppState>) -> CommandResult<()> {
    let mut guard = state.agent.write().await;
    if let Some(server) = guard.take() {
        server.stop().await;
    }
    Ok(())
}

/// 查询 Agent 控制面状态与令牌文件位置。
#[tauri::command(rename_all = "camelCase")]
pub async fn agent_status(state: State<'_, AppState>) -> CommandResult<AgentStatusInfo> {
    let guard = state.agent.read().await;
    Ok(AgentStatusInfo {
        running: guard.is_some(),
        address: guard.as_ref().map(|s| s.address()),
        token_path: fox_agent::token::token_path(&fox_storage::db::data_dir()).display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IPC 契约回归：AgentStatusInfo 按 snake_case 序列化
    /// （token_path 不得变成 tokenPath，与全仓响应命名惯例一致）。
    #[test]
    fn agent_status_serializes_snake_case() {
        let info = AgentStatusInfo {
            running: false,
            address: None,
            token_path: "/tmp/agent-token".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert!(json.get("token_path").is_some(), "缺少字段 token_path");
        assert!(json.get("tokenPath").is_none(), "不得输出 camelCase 字段");
        assert_eq!(json.get("running"), Some(&serde_json::Value::Bool(false)));
    }
}
