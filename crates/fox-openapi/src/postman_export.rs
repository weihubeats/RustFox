//! Postman Collection v2.1 导出。
//!
//! 与 `postman.rs`（导入）对应：把接口集合序列化为可被 Postman / Apifox
//! 一键导入的 Collection v2.1 JSON。请求体取自 BodySpec 样本，
//! 响应示例挂到 item 的 `response` 数组（含 `_postman_previewlanguage`）。

use std::collections::HashMap;

use serde_json::{json, Value};
use uuid::Uuid;

use fox_core::model::{AuthSpec, BodySpec, Endpoint, KeyValue, ResponseExample};

/// 单个接口 → Postman Item。
fn endpoint_to_item(ep: &Endpoint, examples: &[ResponseExample]) -> Value {
    let mut request = json!({
        "method": ep.method.as_str(),
        "header": enabled_kv(&ep.request.headers)
            .into_iter()
            .map(|kv| {
                json!({
                    "key": kv.key,
                    "value": kv.value,
                    "description": if kv.description.is_empty() { None } else { Some(kv.description.clone()) },
                })
            })
            .collect::<Vec<_>>(),
        "url": url_object(&ep.path, &ep.request.params),
        "description": if ep.description.is_empty() { None } else { Some(ep.description.clone()) },
    });
    attach_body(&mut request, &ep.request.body);
    attach_auth(&mut request, &ep.request.auth);

    json!({
        "name": if ep.name.is_empty() {
            format!("{} {}", ep.method.as_str(), ep.path)
        } else {
            ep.name.clone()
        },
        "request": request,
        "response": examples.iter().map(example_to_response).collect::<Vec<_>>(),
    })
}

fn enabled_kv(list: &[KeyValue]) -> Vec<&KeyValue> {
    list.iter()
        .filter(|kv| kv.enabled && !kv.key.trim().is_empty())
        .collect()
}

/// URL 对象：raw 为模板路径，query 携带启用的参数（host 由导入方补齐）。
fn url_object(path: &str, params: &[KeyValue]) -> Value {
    let query: Vec<Value> = enabled_kv(params)
        .into_iter()
        .map(|kv| {
            json!({
                "key": kv.key,
                "value": kv.value,
                "description": if kv.description.is_empty() { None } else { Some(kv.description.clone()) },
            })
        })
        .collect();
    let segments: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    json!({
        "raw": path,
        "path": segments,
        "query": query,
    })
}

fn attach_body(request: &mut Value, body: &BodySpec) {
    let body_value = match body {
        BodySpec::Json { raw } if !raw.trim().is_empty() => Some(json!({
            "mode": "raw",
            "raw": raw,
            "options": { "raw": { "language": "json" } },
        })),
        BodySpec::Text { raw } if !raw.trim().is_empty() => {
            Some(json!({ "mode": "raw", "raw": raw }))
        }
        BodySpec::UrlEncoded { fields } => {
            let rows: Vec<Value> = enabled_kv(fields)
                .into_iter()
                .map(|f| json!({ "key": f.key, "value": f.value }))
                .collect();
            (!rows.is_empty()).then(|| json!({ "mode": "urlencoded", "urlencoded": rows }))
        }
        _ => None,
    };
    if let Some(b) = body_value {
        request["body"] = b;
    }
}

fn attach_auth(request: &mut Value, auth: &AuthSpec) {
    match auth {
        AuthSpec::Bearer { token } if !token.is_empty() => {
            request["auth"] = json!({
                "type": "bearer",
                "bearer": [{ "key": "token", "value": token, "type": "string" }],
            });
        }
        AuthSpec::Basic { username, password } if !username.is_empty() || !password.is_empty() => {
            request["auth"] = json!({
                "type": "basic",
                "basic": [
                    { "key": "username", "value": username, "type": "string" },
                    { "key": "password", "value": password, "type": "string" },
                ],
            });
        }
        AuthSpec::ApiKey {
            key,
            value,
            location: _,
        } if !key.trim().is_empty() && !value.is_empty() => {
            request["auth"] = json!({
                "type": "apikey",
                "apikey": [
                    { "key": "key", "value": key, "type": "string" },
                    { "key": "value", "value": value, "type": "string" },
                    { "key": "in", "value": "header", "type": "string" },
                ],
            });
        }
        _ => {}
    }
}

fn example_to_response(ex: &ResponseExample) -> Value {
    let language = if ex.content_type.contains("json") {
        "json"
    } else {
        "text"
    };
    json!({
        "name": ex.name,
        "originalRequest": {
            "method": "GET",
            "url": { "raw": "", "path": [] },
        },
        "status": "",
        "code": ex.status,
        "_postman_previewlanguage": language,
        "header": ex.headers
            .iter()
            .map(|(k, v)| json!({ "key": k, "value": v }))
            .collect::<Vec<_>>(),
        "cookie": [],
        "body": ex.body,
    })
}

/// 将一组接口导出为 Postman Collection v2.1 JSON 文本。
pub fn export_postman(
    project_name: &str,
    endpoints: &[Endpoint],
    examples_by_endpoint: &HashMap<Uuid, Vec<ResponseExample>>,
) -> String {
    let items: Vec<Value> = endpoints
        .iter()
        .map(|ep| {
            let empty: Vec<ResponseExample> = Vec::new();
            let examples = examples_by_endpoint.get(&ep.id).unwrap_or(&empty);
            endpoint_to_item(ep, examples)
        })
        .collect();

    let collection = json!({
        "info": {
            "name": project_name,
            "description": format!("{project_name} — 由 RustFox 导出"),
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
        },
        "item": items,
    });
    serde_json::to_string_pretty(&collection).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use fox_core::model::{EndpointStatus, HttpMethod, RequestSpec};

    fn sample() -> Endpoint {
        Endpoint {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            folder_id: None,
            name: "创建订单".into(),
            method: HttpMethod::POST,
            path: "/api/v1/orders".into(),
            description: "下单".into(),
            status: EndpointStatus::Released,
            sort_order: 0,
            request: RequestSpec {
                headers: vec![KeyValue {
                    key: "X-Trace".into(),
                    value: "t".into(),
                    enabled: true,
                    description: String::new(),
                    field_type: Default::default(),
                    required: true,
                    example: String::new(),
                }],
                params: vec![KeyValue {
                    key: "dryRun".into(),
                    value: "true".into(),
                    enabled: true,
                    description: String::new(),
                    field_type: Default::default(),
                    required: false,
                    example: String::new(),
                }],
                body: BodySpec::Json {
                    raw: "{\"sku\":\"a\"}".into(),
                },
                ..RequestSpec::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn example_for(ep: &Endpoint) -> ResponseExample {
        ResponseExample {
            id: Uuid::new_v4(),
            endpoint_id: ep.id,
            name: "成功".into(),
            status: 200,
            headers: Default::default(),
            body: "{\"code\":0}".into(),
            content_type: "application/json".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn postman_collection_shape() {
        let ep = sample();
        let mut examples = HashMap::new();
        examples.insert(ep.id, vec![example_for(&ep)]);

        let text = export_postman("演示项目", std::slice::from_ref(&ep), &examples);
        let v: Value = serde_json::from_str(&text).unwrap();

        assert_eq!(
            v["info"]["schema"],
            "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        );
        assert_eq!(v["item"].as_array().unwrap().len(), 1);

        let item = &v["item"][0];
        assert_eq!(item["name"], "创建订单");
        assert_eq!(item["request"]["method"], "POST");
        // Postman v2.1：JSON 体为 raw 模式 + language=json
        assert_eq!(item["request"]["body"]["mode"], "raw");
        assert_eq!(
            item["request"]["body"]["options"]["raw"]["language"],
            "json"
        );
        assert_eq!(item["request"]["header"][0]["key"], "X-Trace");
        assert_eq!(item["request"]["url"]["query"][0]["key"], "dryRun");
        assert_eq!(item["response"][0]["code"], 200);
        assert_eq!(item["response"][0]["_postman_previewlanguage"], "json");
    }

    #[test]
    fn disabled_fields_are_skipped() {
        let mut ep = sample();
        ep.request.params[0].enabled = false;
        let text = export_postman("演示项目", &[ep], &HashMap::new());
        assert!(!text.contains("dryRun"));
    }
}
