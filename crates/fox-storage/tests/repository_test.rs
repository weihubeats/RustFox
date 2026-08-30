//! fox-storage 集成测试：Project / Folder / Endpoint / Environment CRUD。

use sqlx::SqlitePool;

use fox_storage::db::memory_pool;
use fox_storage::repository as repo;

use fox_core::model::{EnvironmentVariable, ModuleUrlConfig, WsMessageType};

async fn pool() -> SqlitePool {
    memory_pool().await.unwrap()
}

#[tokio::test]
async fn project_crud() {
    let db = pool().await;

    let created = repo::create_project(&db, "Demo API", "描述").await.unwrap();
    assert_eq!(created.name, "Demo API");
    assert!(!created.id.to_string().is_empty());

    let listed = repo::list_projects(&db).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
    assert_eq!(listed[0].description, "描述");

    let fetched = repo::get_project(&db, created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert!(created.variables.is_empty());

    let mut updated = fetched.clone();
    updated.name = "改名".into();
    updated
        .variables
        .insert("base_url".into(), "https://x.com".into());
    repo::update_project(&db, &updated).await.unwrap();
    let refetched = repo::get_project(&db, created.id).await.unwrap();
    assert_eq!(refetched.name, "改名");
    assert_eq!(refetched.variables["base_url"], "https://x.com");

    repo::delete_project(&db, created.id).await.unwrap();
    assert!(repo::list_projects(&db).await.unwrap().is_empty());
    assert!(repo::get_project(&db, created.id).await.is_err());
}

#[tokio::test]
async fn folder_crud() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();

    let root = repo::create_folder(&db, project.id, None, "根目录")
        .await
        .unwrap();
    assert_eq!(root.name, "根目录");
    assert!(root.parent_id.is_none());

    let child = repo::create_folder(&db, project.id, Some(root.id), "子目录")
        .await
        .unwrap();
    assert_eq!(child.parent_id, Some(root.id));

    let listed = repo::list_folders(&db, project.id).await.unwrap();
    assert_eq!(listed.len(), 2);

    let fetched = repo::get_folder(&db, child.id).await.unwrap();
    assert_eq!(fetched.name, "子目录");

    // 重命名文件夹。
    let mut renamed = fetched.clone();
    renamed.name = "改名字目录".into();
    let updated = repo::update_folder(&db, &renamed).await.unwrap();
    assert_eq!(updated.name, "改名字目录");
    let fetched = repo::get_folder(&db, child.id).await.unwrap();
    assert_eq!(fetched.name, "改名字目录");

    repo::delete_folder(&db, root.id).await.unwrap();
    // 删除父文件夹后，子文件夹（及整个子树）应一并级联删除。
    assert!(repo::get_folder(&db, child.id).await.is_err());
}

#[tokio::test]
async fn delete_folder_cascades_subtree() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();

    let root = repo::create_folder(&db, project.id, None, "根")
        .await
        .unwrap();
    let child = repo::create_folder(&db, project.id, Some(root.id), "子")
        .await
        .unwrap();
    let grand = repo::create_folder(&db, project.id, Some(child.id), "孙")
        .await
        .unwrap();

    let ep_root = repo::create_endpoint(&db, project.id, Some(root.id), "R")
        .await
        .unwrap();
    let ep_child = repo::create_endpoint(&db, project.id, Some(child.id), "C")
        .await
        .unwrap();
    let ep_grand = repo::create_endpoint(&db, project.id, Some(grand.id), "G")
        .await
        .unwrap();
    let ep_free = repo::create_endpoint(&db, project.id, None, "F")
        .await
        .unwrap();

    repo::delete_folder(&db, root.id).await.unwrap();

    // 子孙文件夹全部删除，不再有孤儿记录。
    assert!(repo::get_folder(&db, root.id).await.is_err());
    assert!(repo::get_folder(&db, child.id).await.is_err());
    assert!(repo::get_folder(&db, grand.id).await.is_err());
    // 子树下接口全部删除。
    assert!(repo::get_endpoint(&db, ep_root.id).await.is_err());
    assert!(repo::get_endpoint(&db, ep_child.id).await.is_err());
    assert!(repo::get_endpoint(&db, ep_grand.id).await.is_err());
    // 子树外接口不受影响。
    assert!(repo::get_endpoint(&db, ep_free.id).await.is_ok());
    // 删除不存在的文件夹返回 NotFound。
    assert!(repo::delete_folder(&db, uuid::Uuid::new_v4())
        .await
        .is_err());
}

#[tokio::test]
async fn endpoint_crud() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();
    let folder = repo::create_folder(&db, project.id, None, "F")
        .await
        .unwrap();

    let created = repo::create_endpoint(&db, project.id, Some(folder.id), "查询用户")
        .await
        .unwrap();
    assert_eq!(created.folder_id, Some(folder.id));
    assert_eq!(created.method.to_string(), "GET");
    assert_eq!(created.status.as_str(), "developing");

    let fetched = repo::get_endpoint(&db, created.id).await.unwrap();
    assert_eq!(fetched.name, "查询用户");
    assert_eq!(fetched.request.params.len(), 0);

    let mut updated = fetched.clone();
    updated.method = "POST".parse().unwrap();
    updated.path = "/users".into();
    updated
        .request
        .params
        .push(fox_core::model::KeyValue::new("page", "1"));
    updated.request.body = fox_core::model::BodySpec::Json {
        raw: "{\"a\":1}".into(),
    };
    let saved = repo::update_endpoint(&db, &updated).await.unwrap();
    assert_eq!(saved.method.to_string(), "POST");

    let refetched = repo::get_endpoint(&db, created.id).await.unwrap();
    assert_eq!(refetched.path, "/users");
    assert_eq!(refetched.request.params[0].key, "page");
    assert_eq!(refetched.request.body.mode_name(), "json");

    let dup = repo::duplicate_endpoint(&db, created.id).await.unwrap();
    assert_ne!(dup.id, created.id);
    assert_eq!(dup.name, format!("{}（副本）", created.name));
    assert_eq!(dup.path, "/users");

    let listed = repo::list_endpoints(&db, project.id).await.unwrap();
    assert_eq!(listed.len(), 2);

    repo::delete_endpoint(&db, created.id).await.unwrap();
    assert!(repo::get_endpoint(&db, created.id).await.is_err());
    assert_eq!(
        repo::list_endpoints(&db, project.id).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn environment_crud() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();

    let env = repo::create_environment(&db, "local", &[], &[])
        .await
        .unwrap();
    assert_eq!(env.name, "local");

    let mut updated = env.clone();
    updated.modules.push(ModuleUrlConfig {
        module_name: "支付".into(),
        base_url: "https://pay.example.com".into(),
        is_default: true,
        ..Default::default()
    });
    updated.modules.push(ModuleUrlConfig {
        module_name: "收单".into(),
        base_url: "https://acq.example.com".into(),
        is_default: false,
        ..Default::default()
    });
    updated.variables.push(EnvironmentVariable {
        key: "token".into(),
        remote_value: "abc".into(),
        local_value: String::new(),
        enabled: true,
        description: None,
    });
    repo::update_environment(&db, &updated).await.unwrap();

    let fetched = repo::get_environment(&db, env.id).await.unwrap();
    // 支付/收单为手工模块；项目「P」被自动同步追加为第三个模块。
    assert_eq!(fetched.modules.len(), 3);
    assert_eq!(fetched.modules[0].base_url, "https://pay.example.com");
    assert!(fetched.modules[0].is_default);
    assert_eq!(fetched.modules[1].module_name, "收单");
    let project_module = fetched
        .modules
        .iter()
        .find(|m| m.project_id.is_some())
        .unwrap();
    assert_eq!(
        project_module.module_name, project.name,
        "项目模块名自动跟随项目名"
    );
    assert_eq!(project_module.base_url, "", "新项目模块基址留空待补填");
    assert_eq!(fetched.variables.len(), 1);
    assert_eq!(fetched.variables[0].effective_value(), "abc");
    // 默认模块基址
    assert_eq!(
        fetched.base_url(None, None),
        Some("https://pay.example.com")
    );
    // 按模块名解析
    assert_eq!(
        fetched.base_url(Some("收单"), None),
        Some("https://acq.example.com")
    );

    let listed = repo::list_environments(&db).await.unwrap();
    assert_eq!(listed.len(), 1);

    // M11：落库应为密文（不包含明文 token），且不含加密格式前缀（明文容错路径）
    let raw: (String,) = sqlx::query_as("SELECT variables_json FROM environments WHERE id = ?")
        .bind(env.id.to_string())
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(
        !raw.0.contains("abc"),
        "变量应加密存储，明文出现在库中: {}",
        raw.0
    );
    assert!(raw.0.contains(':'), "密文应为 base64:base64 格式");
    // 模块基址非敏感信息，明文落库。
    let modules: (String,) = sqlx::query_as("SELECT modules_json FROM environments WHERE id = ?")
        .bind(env.id.to_string())
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(modules.0.contains("https://pay.example.com"));

    repo::delete_environment(&db, env.id).await.unwrap();
    assert!(repo::list_environments(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn cascade_delete_project() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();
    let folder = repo::create_folder(&db, project.id, None, "F")
        .await
        .unwrap();
    let ep = repo::create_endpoint(&db, project.id, Some(folder.id), "E")
        .await
        .unwrap();
    let env = repo::create_environment(&db, "E", &[], &[]).await.unwrap();
    assert_eq!(
        repo::list_endpoints(&db, project.id).await.unwrap().len(),
        1
    );

    let other = repo::create_project(&db, "Q", "").await.unwrap();
    repo::create_endpoint(&db, other.id, None, "X")
        .await
        .unwrap();

    repo::delete_project(&db, project.id).await.unwrap();

    assert!(repo::get_endpoint(&db, ep.id).await.is_err());
    assert!(repo::get_folder(&db, folder.id).await.is_err());
    // 环境为全局维度：不随项目删除级联。
    assert!(repo::get_environment(&db, env.id).await.is_ok());
    assert_eq!(repo::list_projects(&db).await.unwrap().len(), 1);
}

#[tokio::test]
async fn settings_roundtrip() {
    let db = pool().await;
    assert!(repo::get_setting(&db, "k").await.unwrap().is_none());
    repo::set_setting(&db, "port", "4010").await.unwrap();
    assert_eq!(
        repo::get_setting(&db, "port").await.unwrap(),
        Some("4010".into())
    );
    repo::set_setting(&db, "port", "4011").await.unwrap();
    assert_eq!(
        repo::get_setting(&db, "port").await.unwrap(),
        Some("4011".into())
    );
}

#[tokio::test]
async fn ws_message_enqueue_list_delete() {
    let db = pool().await;

    repo::enqueue_ws_message(&db, "ws://a", WsMessageType::Text, "hello")
        .await
        .unwrap();
    repo::enqueue_ws_message(&db, "ws://a", WsMessageType::Binary, "AQID")
        .await
        .unwrap();
    // 其它目标地址互不影响。
    repo::enqueue_ws_message(&db, "ws://b", WsMessageType::Ping, "p1")
        .await
        .unwrap();

    let list = repo::list_pending_ws_messages(&db, "ws://a").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].message_type, WsMessageType::Text);
    assert_eq!(list[0].payload, "hello");
    assert_eq!(list[1].message_type, WsMessageType::Binary);
    assert_eq!(list[1].payload, "AQID");

    repo::delete_ws_messages(&db, &[list[0].id]).await.unwrap();
    let after = repo::list_pending_ws_messages(&db, "ws://a").await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, list[1].id);
    assert_eq!(
        repo::list_pending_ws_messages(&db, "ws://b")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn ws_message_purges_expired() {
    let db = pool().await;
    let record = repo::enqueue_ws_message(&db, "ws://a", WsMessageType::Text, "old")
        .await
        .unwrap();
    // 把记录改到 48 小时前，模拟过期消息。
    let old = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    sqlx::query("UPDATE ws_messages SET created_at = ? WHERE id = ?")
        .bind(old)
        .bind(record.id.to_string())
        .execute(&db)
        .await
        .unwrap();

    // 24 小时内的新消息不受影响。
    repo::enqueue_ws_message(&db, "ws://a", WsMessageType::Text, "fresh")
        .await
        .unwrap();

    let removed = repo::purge_expired_ws_messages(&db, "ws://a", chrono::Duration::hours(24))
        .await
        .unwrap();
    assert_eq!(removed, 1);
    let list = repo::list_pending_ws_messages(&db, "ws://a").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].payload, "fresh");
}

#[tokio::test]
async fn save_folder_repeated_id_updates_not_conflicts() {
    let db = pool().await;
    let project = repo::create_project(&db, "P", "").await.unwrap();

    let created = repo::create_folder(&db, project.id, None, "原名字")
        .await
        .unwrap();
    let mut renamed = created.clone();
    renamed.name = "新名字".into();
    renamed.updated_at = chrono::Utc::now();
    // 重命名走 save_*（带 id 再次保存），此前因主键冲突失败，回归此问题。
    repo::save_folder(&db, &renamed).await.unwrap();

    let fetched = repo::get_folder(&db, created.id).await.unwrap();
    assert_eq!(fetched.name, "新名字");
}

#[tokio::test]
async fn save_project_repeated_id_updates_not_conflicts() {
    let db = pool().await;
    let created = repo::create_project(&db, "原名", "").await.unwrap();
    let mut renamed = created.clone();
    renamed.name = "改名".into();
    renamed.updated_at = chrono::Utc::now();
    repo::save_project(&db, &renamed).await.unwrap();

    let fetched = repo::get_project(&db, created.id).await.unwrap();
    assert_eq!(fetched.name, "改名");
}

#[tokio::test]
async fn save_environment_repeated_id_updates_not_conflicts() {
    let db = pool().await;
    let _project = repo::create_project(&db, "P", "").await.unwrap();
    let created = repo::create_environment(&db, "开发", &[], &[])
        .await
        .unwrap();
    let mut edited = created.clone();
    edited.name = "生产".into();
    edited.updated_at = chrono::Utc::now();
    // save 返回的环境应已同步项目模块（新建环境 + 已存在项目 → 模块自动追加）。
    let saved = repo::save_environment(&db, &edited).await.unwrap();
    assert!(
        saved.modules.iter().any(|m| m.project_id.is_some()),
        "返回环境应含项目模块"
    );
    assert_eq!(saved.name, "生产");

    let fetched = repo::get_environment(&db, created.id).await.unwrap();
    assert_eq!(fetched.name, "生产");
}
