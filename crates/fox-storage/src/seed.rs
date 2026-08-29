//! 开发环境种子数据（仅 debug 构建参与编译）。
//!
//! `npm run tauri dev` 每次启动：`db::reset_dev_database()` 先删库文件，
//! 迁移重建后由本模块写入一批开箱可测的项目 / 接口 / 环境 / Mock 规则，
//! 保证每次重启都是干净一致的测试数据集。release 构建不编译本模块，
//! 正式数据完全不受影响。
//!
//! 数据集以「小奏技术」为演示品牌，项目概览：
//! - 项目 1「小奏技术 · 用户服务」：账号 / 鉴权 REST 接口 + 配套 Mock 规则
//!   （启动 Mock 后基址 http://127.0.0.1:4010 可直接调试）；
//! - 项目 2「小奏技术 · 开放演示」：公网真实 API（JSONPlaceholder），无需 Mock 即可直接发送；
//! - 项目 3「小奏技术 · GraphQL 网关」：公共 GraphQL 服务，测试 GraphQL 工作台；
//! - 环境「开发环境 / 测试环境」（多模块 Base URL + 变量）、全局参数、激活项 settings。

use std::collections::HashMap;

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use fox_core::model::{
    BodySpec, Endpoint, EndpointStatus, Environment, EnvironmentVariable, Folder, GlobalParam,
    GlobalParamLocation, GraphQLSpec, HttpMethod, KeyValue, MockMatchItem, MockRule,
    ModuleUrlConfig, Project, RequestSpec,
};
use fox_core::Result;

use crate::repository as repo;

/// 与 fox-tauri/src/state.rs 的 KEY_ACTIVE_PROJECT / KEY_ACTIVE_ENVIRONMENT 一致；
/// state.rs 未导出常量，此处保持字面量同步。
const KEY_ACTIVE_PROJECT: &str = "active_project_id";
const KEY_ACTIVE_ENVIRONMENT: &str = "active_environment_id";

/// 开发启动时清库后的种子写入入口。
pub async fn seed_dev_data(db: &SqlitePool) -> Result<()> {
    // ---- 项目 ----
    let users = project(
        "小奏技术 · 用户服务",
        "小奏技术内部用户服务演示：账号与鉴权接口。启动 Mock 服务后，\
         开发环境下基址 http://127.0.0.1:4010 的接口可直接调试。",
    );
    let open_demo = project(
        "小奏技术 · 开放演示",
        "小奏技术开放 API 演示：后端指向公网 JSONPlaceholder（jsonplaceholder.typicode.com），\
         无需 Mock 即可直接发送体验。",
    );
    let graphql = project(
        "小奏技术 · GraphQL 网关",
        "小奏技术 GraphQL 网关演示：接入公共 GraphQL 服务（countries.trevorblades.com），用于测试 GraphQL 工作台。",
    );
    for p in [&users, &open_demo, &graphql] {
        repo::save_project(db, p).await?;
    }

    // ---- 用户服务：文件夹 + 接口 + 响应示例 + Mock 规则 ----
    let account_folder = folder(&users, None, "账号管理");
    let auth_folder = folder(&users, None, "鉴权");
    for f in [&account_folder, &auth_folder] {
        repo::save_folder(db, f).await?;
    }

    let list_users = endpoint(
        &users,
        Some(&account_folder),
        "用户列表",
        HttpMethod::GET,
        "/users",
        "分页查询小奏技术账号，Query 参数 page / limit",
        EndpointStatus::Released,
        0,
        RequestSpec {
            params: vec![kv("page", "1"), kv("limit", "10")],
            ..RequestSpec::default()
        },
    );
    let get_user = endpoint(
        &users,
        Some(&account_folder),
        "用户详情",
        HttpMethod::GET,
        "/users/{id}",
        "路径变量 {id}；Mock 规则内置 /users/1 示例",
        EndpointStatus::Released,
        1,
        RequestSpec {
            path_variables: vec![kv("id", "1")],
            ..RequestSpec::default()
        },
    );
    let create_user = endpoint(
        &users,
        Some(&account_folder),
        "创建用户",
        HttpMethod::POST,
        "/users",
        "JSON Body：新增小奏技术账号",
        EndpointStatus::Testing,
        2,
        json_body(r#"{"name": "奏小新", "email": "xinxin@xiaozou.tech", "dept": "设计部"}"#),
    );
    let update_user = endpoint(
        &users,
        Some(&account_folder),
        "更新用户",
        HttpMethod::PUT,
        "/users/{id}",
        "JSON Body + 路径变量",
        EndpointStatus::Developing,
        3,
        json_body(r#"{"name": "奏小雪", "dept": "架构组"}"#),
    );
    let delete_user = endpoint(
        &users,
        Some(&account_folder),
        "删除用户",
        HttpMethod::DELETE,
        "/users/{id}",
        "路径变量 {id}",
        EndpointStatus::Developing,
        4,
        RequestSpec {
            path_variables: vec![kv("id", "1")],
            ..RequestSpec::default()
        },
    );
    let login = endpoint(
        &users,
        Some(&auth_folder),
        "登录",
        HttpMethod::POST,
        "/auth/login",
        "表单（urlencoded）提交 username / password，返回小奏技术 token",
        EndpointStatus::Testing,
        5,
        RequestSpec {
            body: BodySpec::UrlEncoded {
                fields: vec![kv("username", "demo"), kv("password", "xiaozou123")],
            },
            ..RequestSpec::default()
        },
    );
    let me = endpoint(
        &users,
        Some(&auth_folder),
        "当前用户",
        HttpMethod::GET,
        "/auth/me",
        "Bearer {{token}} 认证示例（token 来自环境变量）",
        EndpointStatus::Developing,
        6,
        RequestSpec {
            auth: fox_core::model::AuthSpec::Bearer {
                token: "{{token}}".to_string(),
            },
            ..RequestSpec::default()
        },
    );
    for e in [
        &list_users,
        &get_user,
        &create_user,
        &update_user,
        &delete_user,
        &login,
        &me,
    ] {
        repo::save_endpoint(db, e).await?;
    }

    // 响应示例（文档页展示 + Mock 快速填充的素材）
    for example in [
        response_example(
            &list_users,
            "200 成功",
            200,
            r#"[{"id": 1, "name": "奏小雪", "email": "xuexue@xiaozou.tech", "dept": "研发部"}, {"id": 2, "name": "奏小风", "email": "xiaofeng@xiaozou.tech", "dept": "产品部"}]"#,
        ),
        response_example(
            &get_user,
            "200 成功",
            200,
            r#"{"id": 1, "name": "奏小雪", "email": "xuexue@xiaozou.tech", "dept": "研发部", "company": "小奏技术"}"#,
        ),
        response_example(
            &create_user,
            "201 已创建",
            201,
            r#"{"id": 1001, "name": "奏小新", "email": "xinxin@xiaozou.tech", "dept": "设计部", "company": "小奏技术"}"#,
        ),
        response_example(&delete_user, "204 无内容", 204, ""),
        response_example(
            &login,
            "200 成功",
            200,
            r#"{"token": "xz-mock-token-666", "expires_in": 3600, "company": "小奏技术"}"#,
        ),
    ] {
        repo::save_response_example(db, &example).await?;
    }

    // Mock 规则（启动 Mock 服务后，http://127.0.0.1:4010 直接命中）
    for rule in [
        mock_rule(
            &users,
            Some(&list_users),
            "用户列表",
            HttpMethod::GET,
            "/users",
            200,
            r#"[{"id": 1, "name": "奏小雪", "email": "xuexue@xiaozou.tech", "dept": "研发部"}, {"id": 2, "name": "奏小风", "email": "xiaofeng@xiaozou.tech", "dept": "产品部"}]"#,
            100,
        ),
        mock_rule(
            &users,
            Some(&get_user),
            "用户详情",
            HttpMethod::GET,
            "/users/1",
            200,
            r#"{"id": 1, "name": "奏小雪", "email": "xuexue@xiaozou.tech", "dept": "研发部", "company": "小奏技术"}"#,
            0,
        ),
        mock_rule(
            &users,
            Some(&create_user),
            "创建用户",
            HttpMethod::POST,
            "/users",
            201,
            r#"{"id": 1001, "name": "奏小新", "email": "xinxin@xiaozou.tech", "dept": "设计部", "company": "小奏技术"}"#,
            0,
        ),
        mock_rule(
            &users,
            Some(&login),
            "登录",
            HttpMethod::POST,
            "/auth/login",
            200,
            r#"{"token": "xz-mock-token-666", "expires_in": 3600, "company": "小奏技术"}"#,
            200,
        ),
        mock_rule(
            &users,
            Some(&delete_user),
            "删除用户",
            HttpMethod::DELETE,
            "/users/1",
            204,
            "",
            0,
        ),
    ] {
        repo::save_mock_rule(db, &rule).await?;
    }

    // ---- 开放演示：真实公网接口（JSONPlaceholder） ----
    for (i, (name, method, path, desc, status, request)) in [
        (
            "文章列表",
            HttpMethod::GET,
            "/posts",
            "Query 参数 _limit / _page",
            EndpointStatus::Released,
            RequestSpec {
                params: vec![kv("_limit", "5"), kv("_page", "1")],
                ..RequestSpec::default()
            },
        ),
        (
            "文章详情",
            HttpMethod::GET,
            "/posts/1",
            "按 id 查询",
            EndpointStatus::Released,
            RequestSpec::default(),
        ),
        (
            "发布文章",
            HttpMethod::POST,
            "/posts",
            "JSON Body（服务端返回 201 模拟创建）",
            EndpointStatus::Testing,
            json_body(
                r#"{"title": "小奏技术", "body": "由小奏技术调试工具 RustFox 发送", "userId": 1}"#,
            ),
        ),
        (
            "更新文章",
            HttpMethod::PUT,
            "/posts/1",
            "JSON Body 整体更新",
            EndpointStatus::Developing,
            json_body(r#"{"id": 1, "title": "小奏技术周报", "body": "本周小奏技术动态", "userId": 1}"#),
        ),
        (
            "删除文章",
            HttpMethod::DELETE,
            "/posts/1",
            "服务端返回 200 + 空对象（模拟删除）",
            EndpointStatus::Developing,
            RequestSpec::default(),
        ),
        (
            "用户详情",
            HttpMethod::GET,
            "/users/1",
            "嵌套资源示例",
            EndpointStatus::Released,
            RequestSpec::default(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        repo::save_endpoint(
            db,
            &endpoint(&open_demo, None, name, method, path, desc, status, i as i64, request),
        )
        .await?;
    }

    // ---- GraphQL：公共接口 ----
    for (i, (name, desc, query, variables)) in [
        (
            "国家列表",
            "POST { countries { code name emoji capital } }",
            "query { countries { code name emoji capital } }",
            "",
        ),
        (
            "国家详情（带变量）",
            "带 $code 变量的查询，variables 里改 code 试试",
            "query Country($code: ID!) { country(code: $code) { code name emoji capital currency phone } }",
            r#"{"code": "US"}"#,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        repo::save_endpoint(
            db,
            &endpoint(
                &graphql,
                None,
                name,
                HttpMethod::POST,
                "/graphql",
                desc,
                EndpointStatus::Released,
                i as i64,
                RequestSpec {
                    body: BodySpec::GraphQL {
                        spec: GraphQLSpec {
                            query: query.to_string(),
                            variables: variables.to_string(),
                            operation_name: String::new(),
                        },
                    },
                    ..RequestSpec::default()
                },
            ),
        )
        .await?;
    }

    // ---- 环境（全局维度，多模块 Base URL + 变量） ----
    let dev_env = Environment {
        id: Uuid::new_v4(),
        name: "开发环境".to_string(),
        modules: vec![
            module(&users, "http://127.0.0.1:4010", true),
            module(&open_demo, "https://jsonplaceholder.typicode.com", false),
            module(&graphql, "https://countries.trevorblades.com", false),
        ],
        variables: vec![
            env_var("token", "xz-dev-token-123", "小奏技术登录接口返回的 Bearer Token 示例"),
            env_var("env_name", "development", "当前环境标识"),
            env_var("trace_id", "xz-trace-dev-001", "链路追踪 ID 示例"),
        ],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let staging_env = Environment {
        id: Uuid::new_v4(),
        name: "测试环境".to_string(),
        modules: vec![
            module(&users, "http://127.0.0.1:4010", true),
            module(&open_demo, "https://jsonplaceholder.typicode.com", false),
            module(&graphql, "https://countries.trevorblades.com", false),
        ],
        variables: vec![
            env_var("token", "xz-test-token-456", "小奏技术登录接口返回的 Bearer Token 示例"),
            env_var("env_name", "staging", "当前环境标识"),
            env_var("trace_id", "xz-trace-stg-001", "链路追踪 ID 示例"),
        ],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    repo::save_environment(db, &dev_env).await?;
    repo::save_environment(db, &staging_env).await?;

    // ---- 全局参数（注入制演示） ----
    repo::save_global_params(
        db,
        &[
            GlobalParam {
                key: "X-Client".to_string(),
                value: "XiaoZouTech-RustFox".to_string(),
                enabled: true,
                location: GlobalParamLocation::Header,
            },
            GlobalParam {
                key: "trace_id".to_string(),
                value: "xz-seed-trace-001".to_string(),
                enabled: false,
                location: GlobalParamLocation::Query,
            },
        ],
    )
    .await?;

    // ---- 激活项：启动即落在「小奏技术 · 用户服务」+ 开发环境 ----
    repo::set_setting(db, KEY_ACTIVE_PROJECT, &setting_id(&users.id)).await?;
    repo::set_setting(db, KEY_ACTIVE_ENVIRONMENT, &setting_id(&dev_env.id)).await?;

    Ok(())
}

/* ---------- 构造辅助 ---------- */

fn project(name: &str, description: &str) -> Project {
    let now = Utc::now();
    Project {
        id: Uuid::new_v4(),
        name: name.to_string(),
        description: description.to_string(),
        variables: HashMap::new(),
        created_at: now,
        updated_at: now,
    }
}

fn folder(project: &Project, parent: Option<&Folder>, name: &str) -> Folder {
    let now = Utc::now();
    Folder {
        id: Uuid::new_v4(),
        project_id: project.id,
        parent_id: parent.map(|f| f.id),
        name: name.to_string(),
        sort_order: 0,
        created_at: now,
        updated_at: now,
    }
}

#[allow(clippy::too_many_arguments)]
fn endpoint(
    project: &Project,
    folder: Option<&Folder>,
    name: &str,
    method: HttpMethod,
    path: &str,
    description: &str,
    status: EndpointStatus,
    sort_order: i64,
    request: RequestSpec,
) -> Endpoint {
    let now = Utc::now();
    Endpoint {
        id: Uuid::new_v4(),
        project_id: project.id,
        folder_id: folder.map(|f| f.id),
        name: name.to_string(),
        method,
        path: path.to_string(),
        description: description.to_string(),
        status,
        sort_order,
        request,
        created_at: now,
        updated_at: now,
    }
}

fn response_example(
    endpoint: &Endpoint,
    name: &str,
    status: u16,
    body: &str,
) -> fox_core::model::ResponseExample {
    let now = Utc::now();
    fox_core::model::ResponseExample {
        id: Uuid::new_v4(),
        endpoint_id: endpoint.id,
        name: name.to_string(),
        status,
        headers: HashMap::new(),
        body: body.to_string(),
        content_type: "application/json".to_string(),
        created_at: now,
        updated_at: now,
    }
}

#[allow(clippy::too_many_arguments)]
fn mock_rule(
    project: &Project,
    endpoint: Option<&Endpoint>,
    name: &str,
    method: HttpMethod,
    path: &str,
    status: u16,
    body: &str,
    delay_ms: u64,
) -> MockRule {
    let now = Utc::now();
    MockRule {
        id: Uuid::new_v4(),
        project_id: project.id,
        endpoint_id: endpoint.map(|e| e.id),
        name: name.to_string(),
        method,
        path: path.to_string(),
        match_query: Vec::<MockMatchItem>::new(),
        match_headers: Vec::<MockMatchItem>::new(),
        response_status: status,
        response_headers: HashMap::new(),
        response_body_template: body.to_string(),
        delay_ms,
        enabled: true,
        priority: 0,
        created_at: now,
        updated_at: now,
    }
}

fn module(project: &Project, base_url: &str, is_default: bool) -> ModuleUrlConfig {
    ModuleUrlConfig {
        id: Uuid::new_v4(),
        project_id: Some(project.id),
        module_name: project.name.clone(),
        base_url: base_url.to_string(),
        is_default,
    }
}

fn env_var(key: &str, value: &str, description: &str) -> EnvironmentVariable {
    EnvironmentVariable {
        key: key.to_string(),
        remote_value: value.to_string(),
        local_value: String::new(),
        enabled: true,
        description: Some(description.to_string()),
    }
}

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

fn json_body(raw: &str) -> RequestSpec {
    RequestSpec {
        body: BodySpec::Json {
            raw: raw.to_string(),
        },
        ..RequestSpec::default()
    }
}

/// 与 fox-tauri state.rs `setting_value` 一致：JSON 字符串 `"uuid"`。
fn setting_id(id: &Uuid) -> String {
    serde_json::to_string(&id.to_string()).unwrap_or_else(|_| "null".into())
}
