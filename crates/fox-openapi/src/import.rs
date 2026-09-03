//! OpenAPI 3.0 导入（M7）。

use std::collections::HashMap;
use std::str::FromStr;

use fox_core::model::{BodySpec, HttpMethod, KeyValue, RequestSpec};
use fox_core::AppError;
use indexmap::IndexMap;
use openapiv3::{MediaType, OpenAPI, Parameter, ReferenceOr, RequestBody};
use serde::Serialize;

/// 导入冲突策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictStrategy {
    /// 跳过重复接口（默认）。
    #[default]
    Skip,
    /// 覆盖重复接口。
    Overwrite,
    /// 复制为新接口。
    Duplicate,
}

impl ConflictStrategy {
    pub fn from_str_cn(s: &str) -> Option<Self> {
        match s {
            "skip" => Some(ConflictStrategy::Skip),
            "overwrite" => Some(ConflictStrategy::Overwrite),
            "duplicate" => Some(ConflictStrategy::Duplicate),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictStrategy::Skip => "skip",
            ConflictStrategy::Overwrite => "overwrite",
            ConflictStrategy::Duplicate => "duplicate",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ConflictStrategy::Skip => "跳过",
            ConflictStrategy::Overwrite => "覆盖",
            ConflictStrategy::Duplicate => "复制",
        }
    }
}

/// 导入的响应示例（尚未落库）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportedExample {
    pub name: String,
    pub status: u16,
    pub content_type: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// 导入的接口（尚未落库）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportedEndpoint {
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub description: String,
    pub request: RequestSpec,
    pub examples: Vec<ImportedExample>,
    /// 目标文件夹名（Postman 分组 / OpenAPI tags 兜底），None 表示顶层。
    pub folder_hint: Option<String>,
}

impl ImportedEndpoint {
    fn new(
        name: String,
        method: HttpMethod,
        path: String,
        description: String,
        request: RequestSpec,
        examples: Vec<ImportedExample>,
        folder_hint: Option<String>,
    ) -> Self {
        ImportedEndpoint {
            name,
            method,
            path,
            description,
            request,
            examples,
            folder_hint,
        }
    }
}

/// 支持的导入文档格式（M12；N14 起含 OpenAPI 3.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportFormat {
    OpenApi30,
    OpenApi31,
    Swagger20,
    Postman21,
    Unknown,
}

impl ImportFormat {
    pub fn label(&self) -> &'static str {
        match self {
            ImportFormat::OpenApi30 => "OpenAPI 3.0",
            ImportFormat::OpenApi31 => "OpenAPI 3.1（已转换为 3.0 子集导入）",
            ImportFormat::Swagger20 => "Swagger 2.0",
            ImportFormat::Postman21 => "Postman 集合 v2.1",
            ImportFormat::Unknown => "无法识别",
        }
    }
}

/// 解析文本为 JSON Value（自动识别 JSON / YAML）。
pub(crate) fn parse_value(text: &str) -> Result<serde_json::Value, AppError> {
    let trimmed = text.trim_start();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => Ok(v),
        Err(_) => serde_norway::from_str::<serde_json::Value>(text).map_err(|e| {
            AppError::Validation(format!("文档解析失败，请检查是否为合法的 JSON/YAML：{e}"))
        }),
    }
}

/// 自动识别文档格式（OpenAPI 3.0 / 3.1 / Swagger 2.0 / Postman v2.1）。
pub fn detect_format(text: &str) -> ImportFormat {
    let Ok(v) = parse_value(text) else {
        return ImportFormat::Unknown;
    };
    let Some(root) = v.as_object() else {
        return ImportFormat::Unknown;
    };
    if let Some(version) = root.get("openapi").and_then(|v| v.as_str()) {
        if version.starts_with("3.1") {
            return ImportFormat::OpenApi31;
        }
        return ImportFormat::OpenApi30;
    }
    if root.contains_key("swagger") {
        return ImportFormat::Swagger20;
    }
    if root.contains_key("item") && root.contains_key("info") {
        return ImportFormat::Postman21;
    }
    ImportFormat::Unknown
}

/// 统一入口：自动识别格式并导入，返回接口列表与格式。
pub fn import_any(text: &str) -> Result<(Vec<ImportedEndpoint>, ImportFormat), AppError> {
    let format = detect_format(text);
    let imported = match format {
        ImportFormat::OpenApi30 | ImportFormat::OpenApi31 => import_endpoints(text)?,
        ImportFormat::Swagger20 => crate::swagger2::import_swagger2(text)?,
        ImportFormat::Postman21 => crate::postman::import_postman(text)?,
        ImportFormat::Unknown => return Err(AppError::Validation(
            "无法识别的文档格式：支持 OpenAPI 3.0 / 3.1 / Swagger 2.0 / Postman 集合 v2.1（JSON/YAML）"
                .to_string(),
        )),
    };
    Ok((imported, format))
}

/// 解析 OpenAPI 文本（自动识别 JSON / YAML），校验版本为 3.0 / 3.1。
///
/// 3.1 处理：归一化为 3.0 子集后再走同一套 typed 解析（导入只消费
/// paths/operations/parameters/requestBody/examples，schema 细节无损要求低）：
/// - `openapi: 3.1.x` → `3.0.3`；`webhooks` 顶层键丢弃（给出兼容提示）；
/// - `type: [...]` 数组 → 取首个非 null 类型（nullable 语义由 `enum`/示例承载）；
/// - 数字型 `exclusiveMinimum`/`exclusiveMaximum`（3.1）→ 丢弃（3.0 为布尔型）。
pub fn parse_openapi(text: &str) -> Result<OpenAPI, AppError> {
    let mut value = parse_value(text)?;
    let version = value
        .get("openapi")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if version.starts_with("3.1") {
        normalize_openapi31(&mut value);
    }
    let spec: OpenAPI = serde_json::from_value(value).map_err(|e| {
        AppError::Validation(format!(
            "OpenAPI 文件解析失败，请检查是否为合法的 JSON/YAML：{e}"
        ))
    })?;
    if !spec.openapi.starts_with("3.0") {
        return Err(AppError::Validation(format!(
            "暂不支持 OpenAPI 版本 {}，仅支持 3.0 / 3.1（JSON/YAML）",
            spec.openapi
        )));
    }
    Ok(spec)
}

/// OpenAPI 3.1 → 3.0 子集归一化（原地改 Value）。
fn normalize_openapi31(value: &mut serde_json::Value) {
    use serde_json::Value;
    if let Some(root) = value.as_object_mut() {
        root.insert("openapi".into(), Value::String("3.0.3".into()));
        // webhooks 顶层键：3.0 无对应位置，丢弃（paths 导入不受影响）。
        root.remove("webhooks");
    }
    normalize_schema_node(value);
}

/// 递归归一化 schema 节点（3.1 → 3.0 可解析形状）。
fn normalize_schema_node(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            // type 数组 → 首个非 null 类型。
            if let Some(Value::Array(types)) = map.get("type") {
                let picked = types
                    .iter()
                    .filter_map(|t| t.as_str())
                    .find(|t| *t != "null")
                    .unwrap_or("string")
                    .to_string();
                map.insert("type".into(), Value::String(picked));
            }
            // 数字型 exclusiveMinimum/Maximum → 丢弃（3.0 为布尔型）。
            for key in ["exclusiveMinimum", "exclusiveMaximum"] {
                if map.get(key).is_some_and(|v| v.is_number()) {
                    map.remove(key);
                }
            }
            // const → enum 单元素（3.0 无 const 关键字）。
            if let Some(c) = map.remove("const") {
                map.insert("enum".into(), Value::Array(vec![c]));
            }
            for (_, v) in map.iter_mut() {
                normalize_schema_node(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                normalize_schema_node(v);
            }
        }
        _ => {}
    }
}

/// 解析并导入 OpenAPI 文档，返回待落库的接口列表。
pub fn import_endpoints(text: &str) -> Result<Vec<ImportedEndpoint>, AppError> {
    let spec = parse_openapi(text)?;
    let mut out = Vec::new();
    for (path, item) in &spec.paths.paths {
        let ReferenceOr::Item(path_item) = item else {
            continue;
        };
        for (method_str, op) in path_item.iter() {
            let method = HttpMethod::from_str(method_str)?;
            out.push(map_operation(path, method, op));
        }
    }
    Ok(out)
}

fn map_operation(path: &str, method: HttpMethod, op: &openapiv3::Operation) -> ImportedEndpoint {
    let folder_hint = op.tags.first().cloned();
    let mut params = Vec::new();
    let mut headers = Vec::new();
    let mut path_variables = Vec::new();
    for p in &op.parameters {
        let ReferenceOr::Item(Parameter::Query { parameter_data, .. }) = p else {
            continue;
        };
        params.push(kv_from_data(parameter_data));
    }
    for p in &op.parameters {
        let ReferenceOr::Item(Parameter::Header { parameter_data, .. }) = p else {
            continue;
        };
        headers.push(kv_from_data(parameter_data));
    }
    for p in &op.parameters {
        let ReferenceOr::Item(Parameter::Path { parameter_data, .. }) = p else {
            continue;
        };
        path_variables.push(kv_from_data(parameter_data));
    }

    let body = op
        .request_body
        .as_ref()
        .and_then(|rb| match rb {
            ReferenceOr::Item(rb) => Some(rb),
            ReferenceOr::Reference { .. } => None,
        })
        .map(body_from_request_body)
        .unwrap_or(BodySpec::None);

    let mut examples = Vec::new();
    for (status, resp) in &op.responses.responses {
        let ReferenceOr::Item(r) = resp else {
            continue;
        };
        let status_str = status.to_string();
        let status_num = match status_str.parse::<u16>() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let content = if r.content.is_empty() {
            let mut map = IndexMap::new();
            map.insert("application/json".into(), MediaType::default());
            map
        } else {
            r.content.clone()
        };
        let (content_type, media) = content
            .iter()
            .next()
            .map(|(ct, m)| (ct.clone(), m.clone()))
            .unwrap();
        let mut ex_headers = HashMap::new();
        for (name, h) in &r.headers {
            if let ReferenceOr::Item(h) = h {
                if let Some(v) = &h.example {
                    ex_headers.insert(name.clone(), value_to_string(v));
                }
            }
        }
        examples.push(ImportedExample {
            name: format!("{status_str} {content_type}"),
            status: status_num,
            content_type,
            headers: ex_headers,
            body: media
                .example
                .as_ref()
                .map(value_to_string)
                .unwrap_or_default(),
        });
    }

    let name = op
        .summary
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{method} {path}"));
    let description = op.description.clone().unwrap_or_default();

    ImportedEndpoint::new(
        name,
        method,
        path.to_string(),
        description,
        RequestSpec {
            params,
            headers,
            path_variables,
            auth: Default::default(),
            body,
            ..Default::default()
        },
        examples,
        folder_hint,
    )
}

fn kv_from_data(data: &openapiv3::ParameterData) -> KeyValue {
    KeyValue {
        key: data.name.clone(),
        value: data
            .example
            .as_ref()
            .map(value_to_string)
            .unwrap_or_default(),
        enabled: true,
        description: data.description.clone().unwrap_or_default(),
        field_type: Default::default(),
        required: data.required,
        example: String::new(),
    }
}

fn body_from_request_body(rb: &RequestBody) -> BodySpec {
    for (content_type, media) in &rb.content {
        if content_type.contains("json") {
            let raw = media
                .example
                .as_ref()
                .map(value_to_string)
                .unwrap_or_else(|| "{}".into());
            return BodySpec::Json { raw };
        }
    }
    for (content_type, media) in &rb.content {
        if content_type.contains("x-www-form-urlencoded") {
            let mut fields = Vec::new();
            if let Some(serde_json::Value::Object(map)) = &media.example {
                for (k, v) in map {
                    fields.push(KeyValue::new(k.clone(), value_to_string(v)));
                }
            }
            return BodySpec::UrlEncoded { fields };
        }
    }
    BodySpec::None
}

/// 示例值转字符串。
pub fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JSON: &str = r#"{
  "openapi": "3.0.3",
  "info": { "title": "Demo API", "version": "1.0.0" },
  "paths": {
    "/users": {
      "get": {
        "summary": "用户列表",
        "parameters": [
          { "name": "page", "in": "query", "schema": { "type": "integer" }, "example": 1 },
          { "name": "X-Token", "in": "header", "schema": { "type": "string" }, "example": "abc" }
        ],
        "responses": {
          "200": {
            "description": "ok",
            "content": {
              "application/json": { "example": { "items": [1, 2] } }
            }
          }
        }
      },
      "post": {
        "requestBody": {
          "content": {
            "application/json": { "example": { "name": "张三" } }
          }
        },
        "responses": {
          "201": { "description": "created" }
        }
      }
    },
    "/users/{id}": {
      "get": {
        "parameters": [
          { "name": "id", "in": "path", "required": true, "schema": { "type": "integer" }, "example": 10 }
        ],
        "responses": { "404": { "description": "not found" } }
      }
    }
  }
}"#;

    #[test]
    fn import_json_endpoints() {
        let eps = import_endpoints(SAMPLE_JSON).expect("import should succeed");
        assert_eq!(eps.len(), 3);
        let get = eps
            .iter()
            .find(|e| e.method == HttpMethod::GET && e.path == "/users")
            .unwrap();
        assert_eq!(get.name, "用户列表");
        assert_eq!(get.request.params.len(), 1);
        assert_eq!(get.request.params[0].key, "page");
        assert_eq!(get.request.params[0].value, "1");
        assert_eq!(get.request.headers[0].key, "X-Token");
        assert_eq!(get.request.headers[0].value, "abc");
        assert_eq!(get.examples.len(), 1);
        assert_eq!(get.examples[0].status, 200);
        assert!(get.examples[0].body.contains("items"));
        assert_eq!(get.examples[0].content_type, "application/json");
    }

    #[test]
    fn import_json_body_and_path_vars() {
        let eps = import_endpoints(SAMPLE_JSON).unwrap();
        let post = eps.iter().find(|e| e.method == HttpMethod::POST).unwrap();
        match &post.request.body {
            BodySpec::Json { raw } => assert!(raw.contains("张三")),
            other => panic!("expected json body, got {:?}", other),
        }
        let path_get = eps.iter().find(|e| e.path == "/users/{id}").unwrap();
        assert_eq!(path_get.request.path_variables.len(), 1);
        assert_eq!(path_get.request.path_variables[0].value, "10");
    }

    #[test]
    fn import_yaml() {
        let yaml = "openapi: 3.0.0\ninfo:\n  title: Y\n  version: 1.0.0\npaths:\n  /ping:\n    get:\n      responses:\n        '200':\n          description: pong\n";
        let eps = import_endpoints(yaml).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].path, "/ping");
        assert_eq!(eps[0].examples[0].status, 200);
    }

    #[test]
    fn openapi_31_normalized_and_imported() {
        // 空 paths：版本归一化后可解析。
        let doc = r#"{"openapi":"3.1.0","info":{"title":"x","version":"1"},"paths":{}}"#;
        assert!(import_endpoints(doc).unwrap().is_empty());
        assert_eq!(detect_format(doc), ImportFormat::OpenApi31);
        // 典型 3.1 形状：type 数组 + webhooks + 数字 exclusiveMinimum。
        let doc = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "t", "version": "1"},
            "webhooks": {"newPet": {"post": {"responses": {"200": {"description": "ok"}}}}},
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "parameters": [{
                            "name": "limit", "in": "query",
                            "schema": {"type": ["integer", "null"], "exclusiveMinimum": 0}
                        }],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let eps = import_endpoints(&doc.to_string()).unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].path, "/pets");
        assert_eq!(eps[0].name, "GET /pets");
        assert_eq!(eps[0].request.params.len(), 1);
        assert_eq!(
            import_any(&doc.to_string()).unwrap().1,
            ImportFormat::OpenApi31
        );
    }

    #[test]
    fn reject_invalid_document() {
        let doc = "not a spec at all";
        let err = import_endpoints(doc).unwrap_err();
        assert!(err.user_message().contains("解析失败"));
    }

    #[test]
    fn import_empty_paths_ok() {
        let doc = r#"{"openapi":"3.0.3","info":{"title":"x","version":"1"},"paths":{}}"#;
        let eps = import_endpoints(doc).unwrap();
        assert!(eps.is_empty());
    }

    #[test]
    fn conflict_strategy_roundtrip() {
        assert_eq!(ConflictStrategy::default(), ConflictStrategy::Skip);
        assert_eq!(
            ConflictStrategy::from_str_cn("overwrite"),
            Some(ConflictStrategy::Overwrite)
        );
        assert_eq!(ConflictStrategy::from_str_cn("nope"), None);
        assert_eq!(ConflictStrategy::Duplicate.as_str(), "duplicate");
    }

    #[test]
    fn detect_openapi3() {
        let doc = r#"{"openapi":"3.0.3","info":{"title":"t","version":"1"},"paths":{}}"#;
        assert_eq!(detect_format(doc), ImportFormat::OpenApi30);
    }

    #[test]
    fn detect_swagger2() {
        let doc = r#"{"swagger":"2.0","info":{"title":"t","version":"1"},"paths":{}}"#;
        assert_eq!(detect_format(doc), ImportFormat::Swagger20);
    }

    #[test]
    fn detect_postman() {
        let doc = r#"{"info":{"name":"c","schema":"https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},"item":[]}"#;
        assert_eq!(detect_format(doc), ImportFormat::Postman21);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format("hello world"), ImportFormat::Unknown);
        assert_eq!(detect_format(r#"{"foo":1}"#), ImportFormat::Unknown);
    }

    #[test]
    fn detect_yaml_swagger() {
        let yaml = concat!(
            "swagger: \"2.0\"\n",
            "info:\n",
            "  title: t\n",
            "  version: \"1\"\n",
            "paths: {}\n"
        );
        assert_eq!(detect_format(yaml), ImportFormat::Swagger20);
    }

    #[test]
    fn import_any_unknown_format_errors() {
        assert!(import_any("not-a-doc").is_err());
    }
}
