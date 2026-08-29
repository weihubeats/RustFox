//! 项目 JSON 备份与恢复（M10）。
//!
//! 备份 = 单个 JSON 文件，包含项目及全部子对象（含 UUID 引用关系）。
//! 恢复 = 解析备份并重新分配 UUID（新项目），保证不会与现有数据冲突。

use std::collections::HashMap;

use chrono::Utc;
use fox_core::model::{
    Endpoint, Environment, Folder, MockRule, Project, RequestExample, ResponseExample,
};
use fox_core::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 备份文件（顶层）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupFile {
    pub format: String,
    pub schema_version: u32,
    pub exported_at: String,
    pub project: Project,
    pub folders: Vec<Folder>,
    pub endpoints: Vec<Endpoint>,
    pub environments: Vec<Environment>,
    pub mock_rules: Vec<MockRule>,
    pub response_examples: Vec<ResponseExample>,
    /// 请求用例（旧版本备份无此字段，缺失时按空处理）。
    #[serde(default)]
    pub request_examples: Vec<RequestExample>,
}

/// 备份格式标识。
pub const FORMAT: &str = "rustfox-project-backup";
/// 当前 schema 版本（v2：环境多模块 modules + 结构化变量数组）。
pub const SCHEMA_VERSION: u32 = 2;

impl BackupFile {
    pub fn serialize(&self) -> Result<String, AppError> {
        serde_json::to_string_pretty(self).map_err(AppError::Json)
    }

    pub fn parse(text: &str) -> Result<BackupFile, AppError> {
        let mut value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| AppError::Validation(format!("备份文件解析失败：{e}")))?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if (1..=SCHEMA_VERSION as u64).contains(&version) && version < SCHEMA_VERSION as u64 {
            upgrade_v1(&mut value);
            value["schema_version"] = serde_json::json!(SCHEMA_VERSION);
        }
        let file: BackupFile = serde_json::from_value(value)
            .map_err(|e| AppError::Validation(format!("备份文件解析失败：{e}")))?;
        if file.format != FORMAT {
            return Err(fox_core::validation("不是有效的 RustFox 备份文件"));
        }
        if file.schema_version > SCHEMA_VERSION {
            return Err(fox_core::validation(format!(
                "备份文件版本 {} 过新，当前最高支持 {}",
                file.schema_version, SCHEMA_VERSION
            )));
        }
        Ok(file)
    }
}

/// v1 → v2：环境变量从 `{key:value}` map 升级为结构化数组。
///
/// - `base_url` 键抽出为「默认」模块（modules 空时写入）；
/// - 其余键转为 `EnvironmentVariable { remote_value = value, enabled: true }`。
fn upgrade_v1(value: &mut serde_json::Value) {
    let Some(envs) = value
        .get_mut("environments")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for env in envs {
        let object = match env.as_object_mut() {
            Some(o) => o,
            None => continue,
        };
        // 已是结构化数组则无需升级。
        if object
            .get("variables")
            .and_then(serde_json::Value::as_array)
            .is_some()
        {
            continue;
        }
        let Some(vars_obj) = object.get_mut("variables").and_then(|v| v.as_object_mut()) else {
            continue;
        };
        let mut list: Vec<serde_json::Value> = Vec::new();
        let mut base_url: Option<String> = None;
        for (k, v) in vars_obj.iter() {
            if k == "base_url" {
                base_url = Some(
                    v.as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string()),
                );
                continue;
            }
            if k.trim().is_empty() || k.starts_with("{{") || k.starts_with('$') {
                continue;
            }
            list.push(serde_json::json!({
                "key": k,
                "remote_value": v.as_str().map(String::from).unwrap_or_else(|| v.to_string()),
                "local_value": "",
                "enabled": true,
                "description": null,
            }));
        }
        if let Some(base) = base_url {
            object.insert(
                "modules".into(),
                serde_json::json!([{
                    "id": Uuid::new_v4().to_string(),
                    "module_name": "默认",
                    "base_url": base,
                    "is_default": true,
                }]),
            );
        } else {
            object.insert("modules".into(), serde_json::json!([]));
        }
        object.insert("variables".into(), serde_json::Value::Array(list));
    }
}

/// 构建备份文件。
pub fn build_backup(
    project: &Project,
    folders: &[Folder],
    endpoints: &[Endpoint],
    environments: &[Environment],
    mock_rules: &[MockRule],
    response_examples: &[ResponseExample],
    request_examples: &[RequestExample],
) -> BackupFile {
    BackupFile {
        format: FORMAT.to_string(),
        schema_version: SCHEMA_VERSION,
        exported_at: Utc::now().to_rfc3339(),
        project: project.clone(),
        folders: folders.to_vec(),
        endpoints: endpoints.to_vec(),
        environments: environments.to_vec(),
        mock_rules: mock_rules.to_vec(),
        response_examples: response_examples.to_vec(),
        request_examples: request_examples.to_vec(),
    }
}

/// 恢复结果：所有实体均已重映射到新的 UUID，且原本的引用关系保持一致。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Restored {
    pub project: Project,
    pub folders: Vec<Folder>,
    pub endpoints: Vec<Endpoint>,
    pub environments: Vec<Environment>,
    pub mock_rules: Vec<MockRule>,
    pub response_examples: Vec<ResponseExample>,
    pub request_examples: Vec<RequestExample>,
}

/// 恢复：全量重映射 UUID（新项目）。返回值与 `build_backup` 顺序对应。
pub fn restore_backup(file: &BackupFile) -> Restored {
    let mut map: HashMap<Uuid, Uuid> = HashMap::new();

    let new_project_id = Uuid::new_v4();
    map.insert(file.project.id, new_project_id);
    let mut folders: Vec<Folder> = Vec::new();
    for f in &file.folders {
        let new_id = Uuid::new_v4();
        map.insert(f.id, new_id);
        folders.push(Folder {
            id: new_id,
            project_id: new_project_id,
            parent_id: f.parent_id.map(|p| *map.get(&p).unwrap_or(&new_project_id)),
            ..f.clone()
        });
    }

    let mut endpoints: Vec<Endpoint> = Vec::new();
    for e in &file.endpoints {
        let new_id = Uuid::new_v4();
        map.insert(e.id, new_id);
        endpoints.push(Endpoint {
            id: new_id,
            project_id: new_project_id,
            folder_id: e.folder_id.map(|p| *map.get(&p).unwrap_or(&new_id)),
            ..e.clone()
        });
    }

    let mut environments: Vec<Environment> = Vec::new();
    for e in &file.environments {
        environments.push(Environment {
            id: Uuid::new_v4(),
            ..e.clone()
        });
    }

    let mut mock_rules: Vec<MockRule> = Vec::new();
    for r in &file.mock_rules {
        mock_rules.push(MockRule {
            id: Uuid::new_v4(),
            project_id: new_project_id,
            endpoint_id: r.endpoint_id.map(|p| *map.get(&p).unwrap_or(&Uuid::nil())),
            ..r.clone()
        });
    }

    let mut response_examples: Vec<ResponseExample> = Vec::new();
    for e in &file.response_examples {
        response_examples.push(ResponseExample {
            id: Uuid::new_v4(),
            endpoint_id: *map.get(&e.endpoint_id).unwrap_or(&new_project_id),
            ..e.clone()
        });
    }

    let mut request_examples: Vec<RequestExample> = Vec::new();
    for e in &file.request_examples {
        request_examples.push(RequestExample {
            id: Uuid::new_v4(),
            endpoint_id: *map.get(&e.endpoint_id).unwrap_or(&new_project_id),
            ..e.clone()
        });
    }

    Restored {
        project: Project {
            id: new_project_id,
            ..file.project.clone()
        },
        folders,
        endpoints,
        environments,
        mock_rules,
        response_examples,
        request_examples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fox_core::model::{
        BodySpec, EndpointStatus, EnvironmentVariable, HttpMethod, KeyValue, ModuleUrlConfig,
        RequestSpec,
    };
    fn sample_data() -> BackupFile {
        let project = Project {
            id: Uuid::new_v4(),
            name: "示例项目".into(),
            description: "desc".into(),
            variables: HashMap::from([("base_url".into(), "http://127.0.0.1".into())]),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let folder = Folder {
            id: Uuid::new_v4(),
            project_id: project.id,
            parent_id: None,
            name: "用户".into(),
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut request = RequestSpec::default();
        request.params.push(KeyValue::new("page", "1"));
        request.body = BodySpec::Json {
            raw: r#"{"a":1}"#.into(),
        };
        let ep = Endpoint {
            id: Uuid::new_v4(),
            project_id: project.id,
            folder_id: Some(folder.id),
            name: "列表".into(),
            method: HttpMethod::GET,
            path: "/users".into(),
            description: String::new(),
            status: EndpointStatus::Released,
            sort_order: 1,
            request,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let env = Environment {
            id: Uuid::new_v4(),
            name: "测试".into(),
            modules: vec![ModuleUrlConfig {
                id: Uuid::new_v4(),
                project_id: None,
                module_name: "默认".into(),
                base_url: "https://backup.example.com".into(),
                is_default: true,
            }],
            variables: vec![EnvironmentVariable {
                key: "token".into(),
                remote_value: "t1".into(),
                local_value: String::new(),
                enabled: true,
                description: None,
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let rule = MockRule {
            id: Uuid::new_v4(),
            project_id: project.id,
            endpoint_id: Some(ep.id),
            name: "规则".into(),
            method: HttpMethod::GET,
            path: "/users".into(),
            match_query: Vec::new(),
            match_headers: Vec::new(),
            response_status: 200,
            response_headers: HashMap::new(),
            response_body_template: "{}".into(),
            delay_ms: 0,
            enabled: true,
            priority: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let example = ResponseExample {
            id: Uuid::new_v4(),
            endpoint_id: ep.id,
            name: "成功".into(),
            status: 200,
            headers: HashMap::new(),
            body: r#"{"list":[]}"#.into(),
            content_type: "application/json".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut req_example_request = RequestSpec::default();
        req_example_request.params.push(KeyValue::new("page", "2"));
        let req_example = RequestExample {
            id: Uuid::new_v4(),
            endpoint_id: ep.id,
            name: "分页查询".into(),
            request: req_example_request,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        build_backup(
            &project,
            &[folder],
            &[ep],
            &[env],
            &[rule],
            &[example],
            &[req_example],
        )
    }

    #[test]
    fn serialize_parse_roundtrip() {
        let data = sample_data();
        let text = data.serialize().unwrap();
        let parsed = BackupFile::parse(&text).unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn parse_rejects_wrong_format() {
        assert!(BackupFile::parse(r#"{"format":"other"}"#).is_err());
    }

    #[test]
    fn parse_old_backup_without_request_examples_defaults_empty() {
        let mut data = sample_data();
        data.request_examples.clear();
        let text = data.serialize().unwrap();
        let parsed = BackupFile::parse(&text).unwrap();
        assert!(parsed.request_examples.is_empty());
        assert_eq!(parsed.response_examples.len(), 1);
    }

    #[test]
    fn parse_v1_backup_upgrades_env_variables() {
        // 构造 v2 备份 → 还原为 v1 形状（map 变量、无 modules）。
        let data = sample_data();
        let mut v1 = serde_json::json!(data);
        v1["schema_version"] = serde_json::json!(1);
        let env0 = &mut v1["environments"][0];
        env0.as_object_mut().unwrap().remove("modules");
        env0["variables"] = serde_json::json!({
            "base_url": "https://legacy.example.com",
            "token": "t1",
        });
        let parsed = BackupFile::parse(&v1.to_string()).unwrap();
        assert_eq!(parsed.schema_version, SCHEMA_VERSION);
        let env = &parsed.environments[0];
        // base_url → 默认模块
        assert_eq!(env.modules.len(), 1);
        assert!(env.modules[0].is_default);
        assert_eq!(env.modules[0].base_url, "https://legacy.example.com");
        // 其余键 → 结构化变量
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].key, "token");
        assert_eq!(env.variables[0].remote_value, "t1");
        assert!(env.variables[0].enabled);
    }

    #[test]
    fn restore_remaps_all_ids_consistently() {
        let data = sample_data();
        let restored = restore_backup(&data);
        // 新的项目 id
        assert_ne!(restored.project.id, data.project.id);
        assert_eq!(restored.project.name, data.project.name);
        assert_eq!(restored.project.variables, data.project.variables);
        // 文件夹引用新项目
        assert_eq!(restored.folders.len(), 1);
        let f = &restored.folders[0];
        assert_eq!(f.project_id, restored.project.id);
        assert_ne!(f.id, data.folders[0].id);
        // 接口引用新区块
        let ep = &restored.endpoints[0];
        assert_eq!(ep.project_id, restored.project.id);
        assert_eq!(ep.folder_id, Some(f.id));
        assert_eq!(ep.method, HttpMethod::GET);
        assert_eq!(ep.request.params.len(), 1);
        // MockRule 与 ResponseExample 引用新的 endpoint id
        assert_eq!(restored.mock_rules[0].endpoint_id, Some(ep.id));
        assert_eq!(restored.response_examples[0].endpoint_id, ep.id);
        // 环境为全局维度：恢复后保留模块配置（与项目无归属关系）。
        assert_eq!(restored.environments[0].modules.len(), 1);
        assert!(restored.environments[0].modules[0].is_default);
        // 请求用例：引用新 endpoint id、请求快照保持、名称一致
        let req_ex = &restored.request_examples[0];
        assert_eq!(req_ex.endpoint_id, ep.id);
        assert_eq!(req_ex.name, "分页查询");
        assert_eq!(req_ex.request.params[0].key, "page");
        assert_ne!(req_ex.id, data.request_examples[0].id);
        // 无交叉引用残留
        let old_ids: Vec<Uuid> = data.endpoints.iter().map(|e| e.id).collect();
        for new_ep in &restored.endpoints {
            assert!(!old_ids.contains(&new_ep.id));
        }
    }

    #[test]
    fn restore_is_idempotent_shape() {
        let data = sample_data();
        let a = restore_backup(&data);
        let b = restore_backup(&data);
        assert_ne!(a.project.id, b.project.id);
        assert_eq!(a.endpoints[0].name, b.endpoints[0].name);
    }
}
