//! 环境 Command：列表 / 保存 / 激活切换 / 导入导出。
//!
//! 导入导出对标 Postman/Bruno：原来环境只能随整项目备份 JSON 迁移，
//! 无单环境文件互通。本模块提供 RustFox 原生 JSON 与 Postman Environment
//! v1 双格式（自动识别导入）。注意：导出含变量明文（与备份 JSON 一致），
//! 请妥善保管导出文件。

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use fox_core::model::{Environment, EnvironmentVariable, ModuleUrlConfig};
use fox_storage::repository as repo;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// 列出全部环境（全局维度，跨项目共享；模块已按当前项目自动同步）。
#[tauri::command(rename_all = "camelCase")]
pub async fn list_environments(state: State<'_, AppState>) -> CommandResult<Vec<Environment>> {
    repo::list_environments(&state.db).await.map_err(Into::into)
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
    repo::save_environment(&state.db, &environment)
        .await
        .map_err(Into::into)
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

/// 环境交换格式（导出目标 / 导入自动识别）。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvExchangeFormat {
    /// RustFox 原生 JSON（变量 + 多模块 Base URL 全量）。
    RustfoxJson,
    /// Postman Environment v1（`{name, values:[{key,value,enabled}]}`；模块信息丢弃）。
    PostmanJson,
}

/// 环境导出结果：内容 + 建议文件名（前端经目录选择框落盘）。
#[derive(Debug, Clone, Serialize)]
pub struct ExportedEnv {
    pub content: String,
    pub suggested_name: String,
}

/// 导出单个环境（变量以明文落盘，与备份 JSON 口径一致）。
#[tauri::command(rename_all = "camelCase")]
pub async fn export_environment(
    state: State<'_, AppState>,
    environment_id: Uuid,
    format: EnvExchangeFormat,
) -> CommandResult<ExportedEnv> {
    let env = repo::get_environment(&state.db, environment_id).await?;
    let safe_name: String = env
        .name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_whitespace() => '-',
            c => c,
        })
        .collect();
    let safe_name = if safe_name.trim_matches('-').is_empty() {
        "environment".to_string()
    } else {
        safe_name
    };
    let date = chrono::Local::now().format("%Y-%m-%d");
    match format {
        EnvExchangeFormat::RustfoxJson => {
            let content = serde_json::to_string_pretty(&env)
                .map_err(|e| CommandError::with_code("EXPORT", e.to_string()))?;
            Ok(ExportedEnv {
                content,
                suggested_name: format!("rustfox-env-{safe_name}-{date}.json"),
            })
        }
        EnvExchangeFormat::PostmanJson => {
            let values: Vec<serde_json::Value> = env
                .variables
                .iter()
                .map(|v| {
                    serde_json::json!({
                        "key": v.key,
                        "value": v.effective_value(),
                        "enabled": v.enabled,
                        "type": "default",
                    })
                })
                .collect();
            let doc = serde_json::json!({
                "_postman_variable_scope": "environment",
                "name": env.name,
                "values": values,
            });
            let content = serde_json::to_string_pretty(&doc)
                .map_err(|e| CommandError::with_code("EXPORT", e.to_string()))?;
            Ok(ExportedEnv {
                content,
                suggested_name: format!("postman-env-{safe_name}-{date}.json"),
            })
        }
    }
}

/// 导入预览（不落库）：自动识别 RustFox / Postman 格式，返回可直接保存的环境草稿。
/// id 全部重新生成，名称冲突时由前端在保存前改名（此处仅后缀提示）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportedEnv {
    pub format: &'static str,
    pub name: String,
    pub variables: Vec<EnvironmentVariable>,
    pub modules: Vec<ModuleUrlConfig>,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn import_environment(text: String) -> CommandResult<ImportedEnv> {
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| CommandError::validation(format!("不是合法 JSON：{e}")))?;
    // RustFox 原生：含 variables 数组（modules 可选）。
    if value.get("variables").is_some_and(|v| v.is_array()) {
        let mut env: Environment = serde_json::from_value(value)
            .map_err(|e| CommandError::validation(format!("RustFox 环境格式解析失败：{e}")))?;
        if env.name.trim().is_empty() {
            return Err(CommandError::validation("环境名称为空"));
        }
        env.id = Uuid::new_v4();
        let now = chrono::Utc::now();
        env.created_at = now;
        env.updated_at = now;
        for m in &mut env.modules {
            m.id = Uuid::new_v4();
        }
        return Ok(ImportedEnv {
            format: "rustfox",
            name: env.name,
            variables: env.variables,
            modules: env.modules,
        });
    }
    // Postman：values 数组。
    if let Some(values) = value.get("values").and_then(|v| v.as_array()) {
        let name = value
            .get("name")
            .and_then(|n| n.as_str())
            .filter(|n| !n.trim().is_empty())
            .ok_or_else(|| CommandError::validation("Postman 环境缺少 name"))?
            .to_string();
        let mut variables = Vec::with_capacity(values.len());
        for v in values {
            let Some(key) = v.get("key").and_then(|k| k.as_str()) else {
                continue;
            };
            if key.trim().is_empty() {
                continue;
            }
            variables.push(EnvironmentVariable {
                key: key.to_string(),
                remote_value: v
                    .get("value")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                local_value: String::new(),
                enabled: v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
                description: None,
            });
        }
        return Ok(ImportedEnv {
            format: "postman",
            name,
            variables,
            modules: Vec::new(),
        });
    }
    Err(CommandError::validation(
        "无法识别的环境格式（支持 RustFox 环境 JSON / Postman Environment）",
    ))
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
