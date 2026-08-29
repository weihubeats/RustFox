//! Environment CRUD（全局维度）。
//!
//! 环境跨项目共享（无 project_id 归属）。模块与项目自动联动：
//! 读取 / 保存时按「当前全部项目」同步 `modules` —— 每个项目对应一个模块
//! （`project_id` 绑定、`module_name` 随项目名刷新、缺失的自动追加、已删项目的移除），
//! 确保新建项目在每个环境里自动出现，未配置基址时保持空串由用户补填。

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use fox_core::model::{Environment, ModuleUrlConfig};
use fox_core::{AppError, Result};

use super::projects;
use super::rows::{decrypt_env_json, EnvironmentRow};

/// 按当前全部项目同步模块：
/// - 已绑定的项目模块：刷新 module_name（项目改名后自动跟随）；
/// - 新项目自动追加模块（base_url 留空、非默认）；
/// - 项目已删除的模块移除（保留手工临时模块）；
/// - 无默认模块时补足第一个为默认。
pub fn sync_modules_with_projects(
    modules: &mut Vec<ModuleUrlConfig>,
    projects: &[fox_core::model::Project],
) {
    let mut kept: Vec<ModuleUrlConfig> = modules
        .drain(..)
        .filter(|m| match m.project_id {
            Some(pid) => projects.iter().any(|p| p.id == pid),
            None => true,
        })
        .collect();

    for project in projects {
        if let Some(m) = kept.iter_mut().find(|m| m.project_id == Some(project.id)) {
            m.module_name = project.name.clone();
        } else {
            kept.push(ModuleUrlConfig {
                id: Uuid::new_v4(),
                project_id: Some(project.id),
                module_name: project.name.clone(),
                base_url: String::new(),
                is_default: false,
            });
        }
    }

    if !kept.is_empty() && !kept.iter().any(|m| m.is_default) {
        kept[0].is_default = true;
    }
    *modules = kept;
}

pub async fn create_environment(
    db: &SqlitePool,
    name: &str,
    modules: &[ModuleUrlConfig],
    variables: &[fox_core::model::EnvironmentVariable],
) -> Result<Environment> {
    let now = Utc::now();
    let model = Environment {
        id: Uuid::new_v4(),
        name: name.to_string(),
        modules: modules.to_vec(),
        variables: variables.to_vec(),
        created_at: now,
        updated_at: now,
    };
    let row = EnvironmentRow::from_model(&model);
    sqlx::query(
        "INSERT INTO environments (id, name, variables_json, modules_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.name)
    .bind(row.variables_json.clone())
    .bind(row.modules_json.clone())
    .bind(row.created_at.clone())
    .bind(row.updated_at.clone())
    .execute(db)
    .await?;
    Ok(model)
}

pub async fn get_environment(db: &SqlitePool, environment_id: Uuid) -> Result<Environment> {
    let row: Option<EnvironmentRow> = sqlx::query_as(
        "SELECT id, name, variables_json, modules_json, created_at, updated_at
         FROM environments WHERE id = ?",
    )
    .bind(environment_id.to_string())
    .fetch_optional(db)
    .await?;
    let mut env = row
        .map(EnvironmentRow::into_model)
        .transpose()?
        .ok_or_else(|| AppError::NotFound(format!("环境（{environment_id}）")))?;
    let projects = projects::list_projects(db).await?;
    sync_modules_with_projects(&mut env.modules, &projects);
    Ok(env)
}

pub async fn update_environment(db: &SqlitePool, environment: &Environment) -> Result<Environment> {
    let mut updated = environment.clone();
    updated.updated_at = Utc::now();
    let row = EnvironmentRow::from_model(&updated);
    let result = sqlx::query(
        "UPDATE environments SET name = ?, variables_json = ?, modules_json = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&row.name)
    .bind(row.variables_json.clone())
    .bind(row.modules_json.clone())
    .bind(row.updated_at.clone())
    .bind(&row.id)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("环境（{}）", environment.id)));
    }
    Ok(updated)
}

pub async fn delete_environment(db: &SqlitePool, environment_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM environments WHERE id = ?")
        .bind(environment_id.to_string())
        .execute(db)
        .await?;
    Ok(())
}

pub async fn list_environments(db: &SqlitePool) -> Result<Vec<Environment>> {
    let rows: Vec<EnvironmentRow> = sqlx::query_as(
        "SELECT id, name, variables_json, modules_json, created_at, updated_at
         FROM environments ORDER BY created_at",
    )
    .fetch_all(db)
    .await?;
    let projects = projects::list_projects(db).await?;
    let mut out = Vec::with_capacity(rows.len());
    for mut row in rows {
        // 单行密文损坏（主密钥更换 / 数据篡改）不毒化整个列表：
        // 该环境按空变量表返回，前端仍有全局 DECRYPT 去重提示兜底。
        if decrypt_env_json(&row.variables_json).is_err() {
            tracing::warn!(env = %row.name, "环境变量解密失败，该环境变量表按空处理");
            row.variables_json = "[]".into();
        }
        let mut env = row.into_model()?;
        sync_modules_with_projects(&mut env.modules, &projects);
        out.push(env);
    }
    Ok(out)
}

/// 带 id：原样写入环境（upsert，同一 id 重复保存时更新而非报主键冲突）。
///
/// 保存前同样做一次项目模块同步（保证新建项目出现在每个环境的模块里），
/// 返回同步后的完整环境（模块表已含全部项目），供调用方直接回填 UI。
pub async fn save_environment(db: &SqlitePool, env: &Environment) -> Result<Environment> {
    let mut model = env.clone();
    let projects = projects::list_projects(db).await?;
    sync_modules_with_projects(&mut model.modules, &projects);
    let row = EnvironmentRow::from_model(&model);
    sqlx::query(
        "INSERT INTO environments (id, name, variables_json, modules_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            variables_json = excluded.variables_json,
            modules_json = excluded.modules_json,
            updated_at = excluded.updated_at",
    )
    .bind(&row.id)
    .bind(&row.name)
    .bind(row.variables_json.clone())
    .bind(row.modules_json.clone())
    .bind(row.created_at.clone())
    .bind(row.updated_at.clone())
    .execute(db)
    .await?;
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn project(id: Uuid, name: &str) -> fox_core::model::Project {
        let now = Utc::now();
        fox_core::model::Project {
            id,
            name: name.into(),
            description: String::new(),
            variables: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn sync_keeps_linked_refreshes_name_appends_new_removes_deleted() {
        let p1 = project(Uuid::new_v4(), "支付服务");
        let p2 = project(Uuid::new_v4(), "收单服务");
        let deleted_pid = Uuid::new_v4();
        let mut modules = vec![
            ModuleUrlConfig {
                id: Uuid::new_v4(),
                project_id: Some(p1.id),
                module_name: "旧名".into(),
                base_url: "https://pay.example.com".into(),
                is_default: true,
            },
            ModuleUrlConfig {
                id: Uuid::new_v4(),
                project_id: Some(deleted_pid),
                module_name: "已删项目".into(),
                base_url: "https://gone.example.com".into(),
                is_default: false,
            },
            ModuleUrlConfig {
                id: Uuid::new_v4(),
                project_id: None,
                module_name: "临时模块".into(),
                base_url: "https://adhoc.example.com".into(),
                is_default: false,
            },
        ];
        sync_modules_with_projects(&mut modules, &[p1.clone(), p2.clone()]);

        assert_eq!(
            modules.len(),
            3,
            "p1 保留、p2 追加、已删项目移除、临时模块保留"
        );
        let p1m = modules
            .iter()
            .find(|m| m.project_id == Some(p1.id))
            .unwrap();
        assert_eq!(p1m.module_name, "支付服务", "项目改名后模块名自动刷新");
        assert_eq!(p1m.base_url, "https://pay.example.com", "基址保留");
        let p2m = modules
            .iter()
            .find(|m| m.project_id == Some(p2.id))
            .unwrap();
        assert_eq!(p2m.base_url, "", "新项目模块基址留空待补填");
        assert!(!modules.iter().any(|m| m.project_id == Some(deleted_pid)));
        assert!(modules.iter().any(|m| m.project_id.is_none()));
        assert!(
            modules.iter().filter(|m| m.is_default).count() == 1,
            "保证唯一默认模块"
        );
    }
}
