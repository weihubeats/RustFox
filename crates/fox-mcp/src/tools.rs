//! MCP 协议处理：JSON-RPC 分发 + 工具定义 + 工具调用。
//!
//! 只实现 server 必需的最小集：`initialize` / `ping` / `tools/list` /
//! `tools/call`（+ 未知方法 -32601）。协议版本回显客户端请求值，兼容性最好。

use fox_agent::ControlClient;
use fox_core::AppError;
use serde_json::{json, Value};
use uuid::Uuid;

/// 默认协议版本（客户端未携带时使用）。
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

// ---------- 入口 ----------

/// 处理一行请求：自动 `discover_default()` 建立控制面连接。
pub async fn handle_line_discover(line: &str) -> Option<String> {
    handle_line_with(line, || Box::pin(ControlClient::discover_default())).await
}

pub(crate) async fn handle_line_with(
    line: &str,
    connect: impl FnOnce() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ControlClient, AppError>> + Send>,
    >,
) -> Option<String> {
    let msg: Value = serde_json::from_str(line).ok()?;
    let id = msg.get("id").cloned();
    let method = msg.get("method")?.as_str()?.to_string();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    let payload: Result<Value, (i64, String)> = match method.as_str() {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => match connect().await {
            Err(e) => Err(tool_error(&e.user_message())),
            Ok(client) => call_tool(&client, &params).await,
        },
        // 未知 notification：静默忽略；未知 request：协议错误
        _ if id.is_none() => return None,
        _ => Err((-32601, format!("Method not found: {method}"))),
    };

    let mut response = json!({ "jsonrpc": "2.0", "id": id });
    match payload {
        Ok(result) => response["result"] = result,
        Err((code, message)) => {
            response["error"] = json!({ "code": code, "message": message });
        }
    }
    Some(response.to_string())
}

fn initialize_result(params: &Value) -> Value {
    json!({
        "protocolVersion": params["protocolVersion"].as_str().unwrap_or(DEFAULT_PROTOCOL_VERSION),
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "rustfox-mcp", "version": env!("CARGO_PKG_VERSION") }
    })
}

/// 工具执行失败按 MCP 约定返回 `isError: true` 的正常结果（非协议错误）。
fn tool_error(message: &str) -> (i64, String) {
    (-32000, message.to_string())
}

// ---------- 工具定义 ----------

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "save_curl",
            "description": "把一条 cURL 命令解析并保存为 RustFox 接口（API 调试工具中的可编辑接口）。URL 拆为 base_url + 路径 + 查询参数；返回 endpointId。多个项目时必须传 projectId。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "完整 cURL 命令字符串" },
                    "name": { "type": "string", "description": "可选，接口名称；缺省从 URL 路径推导" },
                    "projectId": { "type": "string", "description": "可选，目标项目 UUID；缺省时唯一项目自动选中、零项目自动创建" },
                    "folderId": { "type": "string", "description": "可选，归属文件夹 UUID" }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "list_projects",
            "description": "列出 RustFox 中全部项目（id / 名称 / 描述 / 变量）。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_endpoints",
            "description": "列出指定项目下的全部接口（方法 / 路径 / 名称 / 请求规格）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "projectId": { "type": "string", "description": "项目 UUID（先调 list_projects 获取）" }
                },
                "required": ["projectId"]
            }
        }),
        json!({
            "name": "agent_info",
            "description": "返回 RustFox Agent 控制面连接信息（地址与令牌文件路径），供需要直接发 HTTP 的场景使用。",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

// ---------- 工具调用 ----------

async fn call_tool(client: &ControlClient, params: &Value) -> Result<Value, (i64, String)> {
    let name = params["name"].as_str().unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let value: Value = match name {
        "save_curl" => {
            let command = str_arg(&args, "command")
                .ok_or_else(|| (-32602, "缺少必填参数 command".to_string()))?;
            let project_id = opt_uuid_arg(&args, "projectId").map_err(|e| (-32602, e))?;
            let folder_id = opt_uuid_arg(&args, "folderId").map_err(|e| (-32602, e))?;
            serde_json::to_value(
                client
                    .save_curl(command, str_arg(&args, "name"), project_id, folder_id)
                    .await
                    .map_err(|e| tool_error(&e.user_message()))?,
            )
            .expect("序列化失败")
        }
        "list_projects" => serde_json::to_value(
            client
                .list_projects()
                .await
                .map_err(|e| tool_error(&e.user_message()))?,
        )
        .expect("序列化失败"),
        "list_endpoints" => {
            let project_id: Uuid = str_arg(&args, "projectId")
                .ok_or_else(|| (-32602, "缺少必填参数 projectId".to_string()))?
                .parse()
                .map_err(|_| (-32602, "projectId 不是合法 UUID".to_string()))?;
            serde_json::to_value(
                client
                    .list_endpoints(project_id)
                    .await
                    .map_err(|e| tool_error(&e.user_message()))?,
            )
            .expect("序列化失败")
        }
        "agent_info" => json!({
            "address": client.base(),
            "tokenPath": fox_agent::token::token_path(&fox_agent::default_data_dir()),
            "auth": "Authorization: Bearer <token 文件内容>",
            "docs": "POST /agent/curl | GET /agent/projects | GET /agent/endpoints/:projectId"
        }),
        other => return Err((-32602, format!("未知工具：{other}"))),
    };
    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).expect("格式化失败") }],
        "isError": false
    }))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key].as_str().filter(|s| !s.is_empty())
}

fn opt_uuid_arg(args: &Value, key: &str) -> Result<Option<Uuid>, String> {
    match args[key].as_str() {
        None | Some("") => Ok(None),
        Some(s) => s
            .parse::<Uuid>()
            .map(Some)
            .map_err(|_| format!("{key} 不是合法 UUID")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fox_agent::server::{build_router, AgentState};
    use fox_storage::db::init_db;

    /// 启动临时控制面并构造指向它的 connect 闭包。
    fn test_connect(
        url: String,
    ) -> impl FnOnce() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ControlClient, AppError>> + Send>,
    > {
        move || Box::pin(async move { Ok(ControlClient::new(url, "tok")) })
    }

    #[tokio::test]
    async fn initialize_and_list_tools() {
        let reply = handle_line_with(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            || unreachable!(),
        )
        .await
        .unwrap();
        assert!(reply.contains("\"protocolVersion\":\"2024-11-05\""));
        assert!(reply.contains("rustfox-mcp"));

        let reply = handle_line_with(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            || unreachable!(),
        )
        .await
        .unwrap();
        for tool in ["save_curl", "list_projects", "list_endpoints", "agent_info"] {
            assert!(reply.contains(tool), "缺少工具 {tool}");
        }
    }

    #[tokio::test]
    async fn notification_gets_no_reply() {
        let reply = handle_line_with(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            || unreachable!(),
        )
        .await;
        assert!(reply.is_none());
    }

    #[tokio::test]
    async fn unknown_request_is_method_not_found() {
        let reply = handle_line_with(
            r#"{"jsonrpc":"2.0","id":9,"method":"resources/list"}"#,
            || unreachable!(),
        )
        .await
        .unwrap();
        assert!(reply.contains("-32601"));
    }

    #[tokio::test]
    async fn save_curl_roundtrip_via_mcp() {
        let dir = std::env::temp_dir().join(format!("rustfox-mcp-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = init_db(&dir.join("t.db")).await.unwrap();
        let state = AgentState::new(db.clone(), "tok");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, build_router(state)).await });
        let connect = test_connect(format!("http://{addr}"));

        let line = serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "save_curl",
                        "arguments": { "command": "curl https://api.example.com/users?full=1" } }
        })
        .to_string();
        let reply = handle_line_with(&line, connect).await.unwrap();
        let parsed: Value = serde_json::from_str(&reply).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("endpointId"), "{reply}");
        assert!(text.contains("/users"));
        assert_eq!(parsed["result"]["isError"], false);

        // 缺参数 → 参数错误（connect 在校验前调用，需指向同一服务端）
        let bad = serde_json::json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "save_curl", "arguments": {} }
        })
        .to_string();
        let reply = handle_line_with(&bad, test_connect(format!("http://{addr}")))
            .await
            .unwrap();
        assert!(reply.contains("-32602"), "{reply}");
    }
}
