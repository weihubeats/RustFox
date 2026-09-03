//! 全局状态管理器：SQLite 连接 + 当前激活 Project / Environment 缓存。
//!
//! - 使用 `tokio::sync::RwLock`（读多写少），Command 并发读取激活上下文；
//! - 激活对象首次访问时从数据库加载并写回缓存，避免重复查询；
//! - `variables_for` 提供「环境 > 项目」合并变量表，供请求渲染使用。

use std::collections::HashMap;
use std::sync::Mutex;

use fox_core::model::{Environment, Project};
use fox_core::VariableMap;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use fox_storage::repository as repo;

use crate::error::CommandResult;

/// 激活上下文持久化键（settings 表）。
const KEY_ACTIVE_PROJECT: &str = "active_project_id";
const KEY_ACTIVE_ENVIRONMENT: &str = "active_environment_id";

/// 序列化激活 id 为 settings 值（JSON：`"uuid"` 或 `null`）。
fn setting_value(id: Option<Uuid>) -> String {
    match id {
        Some(id) => serde_json::to_string(&id.to_string()).unwrap_or_else(|_| "null".into()),
        None => "null".into(),
    }
}

/// 读取持久化的激活 id（缺失 / 损坏返回 `None`）。
async fn load_setting_uuid(db: &SqlitePool, key: &str) -> Option<Uuid> {
    repo::get_setting(db, key)
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str::<Option<String>>(&v).ok().flatten())
        .and_then(|s| Uuid::parse_str(&s).ok())
}

/// 当前激活上下文（多标签 / 多窗口共享）。
#[derive(Debug, Default)]
pub struct ActiveContext {
    pub project_id: Option<Uuid>,
    /// 缓存的项目（避免重复查询）。
    pub project: Option<Project>,
    pub environment_id: Option<Uuid>,
    /// 缓存的环境。
    pub environment: Option<Environment>,
}

/// 应用全局状态，由插件在 `setup` 中 `app.manage()` 托管。
pub struct AppState {
    pub db: SqlitePool,
    /// 激活上下文（读写并发安全）。
    pub active: RwLock<ActiveContext>,
    /// 正在运行的 Mock 服务（未启动为 `None`）。
    pub mock: RwLock<Option<fox_mock::server::MockServer>>,
    /// 正在运行的 Agent 控制面服务（未启动为 `None`）。
    pub agent: RwLock<Option<fox_agent::server::AgentServer>>,
    /// 在途请求的取消令牌注册表（request_id → token；「取消请求」时触发中止）。
    /// 持有期间不 await，普通 `Mutex` 即可。
    pub request_cancels: Mutex<HashMap<String, CancellationToken>>,
    /// 在途长任务的取消令牌注册表（run_id → token；
    /// 压测 `cancel_load_test` 与集合测试 `cancel_test_collection` 共用）。
    pub run_cancels: Mutex<HashMap<String, CancellationToken>>,
    /// WebSocket 会话（connection_id → 会话；含事件转发任务句柄）。
    pub ws: RwLock<HashMap<String, crate::commands::ws::WsSession>>,
    /// SSE 订阅任务（connection_id → 转发任务句柄；断开即 abort）。
    pub sse: RwLock<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl AppState {
    pub fn new(db: SqlitePool) -> Self {
        AppState {
            db,
            active: RwLock::new(ActiveContext::default()),
            mock: RwLock::new(None),
            agent: RwLock::new(None),
            request_cancels: Mutex::new(HashMap::new()),
            run_cancels: Mutex::new(HashMap::new()),
            ws: RwLock::new(HashMap::new()),
            sse: RwLock::new(HashMap::new()),
        }
    }

    /// 当前激活项目（缓存命中直接返回；否则查询并写回缓存）。
    pub async fn active_project(&self) -> CommandResult<Option<Project>> {
        let read = self.active.read().await;
        if let Some(project) = &read.project {
            return Ok(Some(project.clone()));
        }
        let Some(id) = read.project_id else {
            return Ok(None);
        };
        drop(read);
        let project = repo::get_project(&self.db, id).await?;
        let mut write = self.active.write().await;
        write.project = Some(project.clone());
        Ok(Some(project))
    }

    /// 当前激活环境（缓存命中直接返回；否则查询并写回缓存）。
    pub async fn active_environment(&self) -> CommandResult<Option<Environment>> {
        let read = self.active.read().await;
        if let Some(environment) = &read.environment {
            return Ok(Some(environment.clone()));
        }
        let Some(id) = read.environment_id else {
            return Ok(None);
        };
        drop(read);
        let environment = repo::get_environment(&self.db, id).await?;
        let mut write = self.active.write().await;
        write.environment = Some(environment.clone());
        Ok(Some(environment))
    }

    /// 设置激活项目（`None` 表示清空）。
    /// 持久化到 settings 表，重启后由 `restore_active` 恢复。
    ///
    /// 环境为全局维度，切换项目不改变激活环境。
    /// 数据库查询全部在锁外完成：写锁若跨 await，会阻塞所有并发读
    /// （每次发请求的 `variables_for` 都要读激活上下文）。
    pub async fn set_active_project(&self, project_id: Option<Uuid>) -> CommandResult<()> {
        let project = match project_id {
            Some(id) => Some(repo::get_project(&self.db, id).await?),
            None => None,
        };

        let mut write = self.active.write().await;
        write.project_id = project_id;
        write.project = project;
        drop(write);
        repo::set_setting(&self.db, KEY_ACTIVE_PROJECT, &setting_value(project_id)).await?;
        Ok(())
    }

    /// 设置激活环境（`None` 表示不使用环境变量）。持久化到 settings 表。
    pub async fn set_active_environment(&self, environment_id: Option<Uuid>) -> CommandResult<()> {
        // 查库在锁外：无效 id 在此返回错误，不占用写锁
        let environment = match environment_id {
            Some(id) => Some(repo::get_environment(&self.db, id).await?),
            None => None,
        };
        let mut write = self.active.write().await;
        write.environment_id = environment_id;
        write.environment = environment;
        drop(write);
        repo::set_setting(
            &self.db,
            KEY_ACTIVE_ENVIRONMENT,
            &setting_value(environment_id),
        )
        .await?;
        Ok(())
    }

    /// 启动时恢复持久化的激活项目 / 环境（校验存在性，无效则丢弃）。
    /// 环境为全局维度，不校验项目归属。
    pub async fn restore_active(&self) -> CommandResult<()> {
        let project_id = load_setting_uuid(&self.db, KEY_ACTIVE_PROJECT).await;
        let project_ok = match project_id {
            Some(id) => repo::get_project(&self.db, id).await.is_ok(),
            None => false,
        };
        let environment_id = load_setting_uuid(&self.db, KEY_ACTIVE_ENVIRONMENT).await;
        let env_ok = match environment_id {
            Some(id) => repo::get_environment(&self.db, id).await.is_ok(),
            None => false,
        };
        let mut write = self.active.write().await;
        write.project_id = project_id.filter(|_| project_ok);
        write.environment_id = environment_id.filter(|_| env_ok);
        write.project = None;
        write.environment = None;
        Ok(())
    }

    /// 合并变量表：运行时（空）> 环境 > 项目 > 全局。
    ///
    /// 环境侧取「结构化变量（enabled、本地值优先）」扁平表；此外当环境中存在
    /// 默认模块时，注入 `base_url = 默认模块前置 URL`（未显式定义 base_url 变量时），
    /// 使请求引擎 `{{base_url}}` 拼接在旧语义下继续可用。
    pub async fn variables_for(&self, environment_id: Option<Uuid>) -> CommandResult<VariableMap> {
        let project = self.active_project().await?;
        let environment = match environment_id {
            // 单次请求显式指定环境：临时加载，不改动全局激活状态。
            Some(id) => Some(repo::get_environment(&self.db, id).await?),
            None => self.active_environment().await?,
        };
        let global_vars: VariableMap = repo::get_global_variables(&self.db)
            .await?
            .into_iter()
            .filter(|v| v.enabled)
            .map(|v| {
                let value = v.effective_value().to_string();
                (v.key, value)
            })
            .collect();
        let project_id = project.as_ref().map(|p| p.id);
        let project_vars = project.map(|p| p.variables).unwrap_or_default();
        let mut environment_vars: VariableMap = environment
            .as_ref()
            .map(|e| e.effective_variables())
            .unwrap_or_default();
        if !environment_vars.contains_key("base_url") {
            // 默认模块随当前激活项目走：开放演示项目注入 jsonplaceholder，
            // 用户服务项目注入 127.0.0.1:4010，而非全局 is_default 钉死的模块。
            if let Some(base) = environment
                .as_ref()
                .and_then(|e| e.base_url(None, project_id))
            {
                environment_vars.insert("base_url".into(), base.to_string());
            }
        }
        // 单次合并（优先级 环境 > 项目 > 全局）：三张表均为 owned，直接 move，
        // 原来两次 `merge_variables` 把全部键值克隆了两遍。
        let mut merged = global_vars;
        for (k, v) in project_vars {
            merged.insert(k, v);
        }
        for (k, v) in environment_vars {
            merged.insert(k, v);
        }
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fox_core::model::{Environment, ModuleUrlConfig, Project};
    use std::path::PathBuf;

    /// {{base_url}} 注入随激活项目走：默认模块优先取当前项目绑定的模块，
    /// 而非全局 is_default 钉死的模块（多项目共用一个环境的场景）。
    #[tokio::test]
    async fn base_url_follows_active_project_module() {
        let path: PathBuf =
            std::env::temp_dir().join(format!("rustfox-module-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = fox_storage::db::init_db(&path).await.expect("建库");
        let state = AppState::new(db.clone());

        let mk_project = |name: &str| Project {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            variables: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let users = mk_project("小奏技术 · 用户服务");
        let open = mk_project("小奏技术 · 开放演示");
        repo::save_project(&db, &users).await.expect("落库项目");
        repo::save_project(&db, &open).await.expect("落库项目");

        let env_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        repo::save_environment(
            &db,
            &Environment {
                id: env_id,
                name: "dev".into(),
                // is_default 钉在开放演示模块上：历史语义会把它注入一切项目
                modules: vec![
                    ModuleUrlConfig {
                        id: Uuid::new_v4(),
                        project_id: Some(users.id),
                        module_name: users.name.clone(),
                        base_url: "http://127.0.0.1:4010".into(),
                        is_default: false,
                    },
                    ModuleUrlConfig {
                        id: Uuid::new_v4(),
                        project_id: Some(open.id),
                        module_name: open.name.clone(),
                        base_url: "https://jsonplaceholder.typicode.com".into(),
                        is_default: true,
                    },
                ],
                variables: Vec::new(),
                created_at: now,
                updated_at: now,
            },
        )
        .await
        .expect("落库环境");
        state
            .set_active_environment(Some(env_id))
            .await
            .expect("激活环境");

        // 激活用户服务 → base_url = 用户服务自己的模块基址
        state
            .set_active_project(Some(users.id))
            .await
            .expect("激活");
        let vars = state.variables_for(None).await.expect("变量表");
        assert_eq!(
            vars.get("base_url").map(String::as_str),
            Some("http://127.0.0.1:4010")
        );

        // 切到开放演示 → base_url 跟随切换（即使另一模块才是 is_default）
        state.set_active_project(Some(open.id)).await.expect("激活");
        let vars = state.variables_for(None).await.expect("变量表");
        assert_eq!(
            vars.get("base_url").map(String::as_str),
            Some("https://jsonplaceholder.typicode.com")
        );
        let _ = std::fs::remove_file(&path);
    }

    /// 激活项目 / 环境必须跨「重启」恢复：写入 settings 表，重建状态后可读回。
    #[tokio::test]
    async fn active_context_persists_across_restart() {
        let path: PathBuf =
            std::env::temp_dir().join(format!("rustfox-active-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = fox_storage::db::init_db(&path).await.expect("建库");
        let state = AppState::new(db.clone());

        let project = Project {
            id: Uuid::new_v4(),
            name: "测试项目".into(),
            description: String::new(),
            variables: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        repo::save_project(&db, &project).await.expect("落库项目");
        let env_id = Uuid::new_v4();
        repo::save_environment(
            &db,
            &Environment {
                id: env_id,
                name: "dev".into(),
                modules: Vec::new(),
                variables: Vec::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("落库环境");

        state
            .set_active_project(Some(project.id))
            .await
            .expect("激活项目");
        state
            .set_active_environment(Some(env_id))
            .await
            .expect("激活环境");

        // 模拟重启：同库新建状态，仅靠 settings 表恢复。
        let restarted = AppState::new(db.clone());
        restarted.restore_active().await.expect("恢复激活上下文");
        let read = restarted.active.read().await;
        assert_eq!(read.project_id, Some(project.id), "项目应恢复");
        assert_eq!(read.environment_id, Some(env_id), "环境应恢复");
        drop(read);
        assert_eq!(
            restarted
                .active_environment()
                .await
                .expect("读环境")
                .map(|e| e.id),
            Some(env_id)
        );

        // 回归：重启后用户经项目列表「重新进入」同一项目，环境必须保留
        //（restore 后 environment 缓存为空，旧逻辑会误判归属并清空环境）。
        restarted
            .set_active_project(Some(project.id))
            .await
            .expect("重进项目");
        {
            let read = restarted.active.read().await;
            assert_eq!(
                read.environment_id,
                Some(env_id),
                "重进同一项目不应清空环境"
            );
        }

        // 环境为全局维度：切换 / 新增其他项目，激活环境保持有效。
        let other_project_id = Uuid::new_v4();
        repo::save_project(
            &db,
            &Project {
                id: other_project_id,
                name: "其他项目".into(),
                description: String::new(),
                variables: Default::default(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        )
        .await
        .expect("落库其他项目");

        // 切换其他项目 → 全局环境不应被清空。
        restarted
            .set_active_project(Some(other_project_id))
            .await
            .expect("切换项目");
        {
            let read = restarted.active.read().await;
            assert_eq!(read.project_id, Some(other_project_id));
            assert_eq!(read.environment_id, Some(env_id), "全局环境跨项目保持");
        }

        // 重启恢复：环境 id 有效即恢复，不受项目切换影响。
        let again = AppState::new(db.clone());
        again.restore_active().await.expect("恢复");
        let read = again.active.read().await;
        assert_eq!(read.project_id, Some(other_project_id));
        assert_eq!(read.environment_id, Some(env_id), "全局环境重启后恢复");
        drop(read);

        db.close().await;
        let _ = std::fs::remove_file(&path);
    }
}
