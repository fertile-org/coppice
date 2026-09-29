//! Minimal MCP (Streamable HTTP, JSON-only) protocol core: JSON-RPC dispatch over a `ToolHost`.

use async_trait::async_trait;
use serde_json::{json, Value};

pub const SERVER_NAME: &str = "coppice";

const SUPPORTED_VERSIONS: [&str; 2] = ["2025-06-18", "2025-03-26"];
const FALLBACK_VERSION: &str = "2025-06-18";

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub read_only: bool,
}

impl ToolDefinition {
    fn to_mcp(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": { "readOnlyHint": self.read_only },
        })
    }
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    fn to_mcp(&self) -> Value {
        json!({
            "content": [{ "type": "text", "text": self.text }],
            "isError": self.is_error,
        })
    }
}

#[async_trait]
pub trait ToolHost: Send + Sync {
    fn list(&self) -> Vec<ToolDefinition>;
    async fn call(&self, name: &str, args: Value) -> ToolOutput;
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Returns `None` for notifications (no `id`), which must not receive a response.
pub async fn handle_rpc(body: Value, host: &dyn ToolHost) -> Option<Value> {
    if body.is_array() {
        return Some(err(
            Value::Null,
            INVALID_REQUEST,
            "batch requests are not supported",
        ));
    }
    let Some(method) = body.get("method").and_then(Value::as_str) else {
        return Some(err(
            body.get("id").cloned().unwrap_or(Value::Null),
            INVALID_REQUEST,
            "missing method",
        ));
    };
    let id = body.get("id").cloned()?;
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            let version = requested
                .filter(|v| SUPPORTED_VERSIONS.contains(v))
                .unwrap_or(FALLBACK_VERSION);
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = host.list().iter().map(ToolDefinition::to_mcp).collect();
            ok(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(err(id, INVALID_PARAMS, "missing tool name"));
            };
            let args = params
                .get("arguments")
                .filter(|a| !a.is_null())
                .cloned()
                .unwrap_or_else(|| json!({}));
            ok(id, host.call(name, args).await.to_mcp())
        }
        _ => err(id, METHOD_NOT_FOUND, "method not found"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::{json, Value};

    struct FakeHost;

    #[async_trait]
    impl ToolHost for FakeHost {
        fn list(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "look".into(),
                description: "Look".into(),
                input_schema: json!({"type": "object"}),
                read_only: true,
            }]
        }
        async fn call(&self, _name: &str, _args: Value) -> ToolOutput {
            ToolOutput {
                text: "hi".into(),
                is_error: true,
            }
        }
    }

    #[tokio::test]
    async fn initialize_echoes_supported_version() {
        let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        assert_eq!(res["id"], 1);
        assert_eq!(res["result"]["protocolVersion"], "2025-03-26");
        assert!(res["result"]["capabilities"]["tools"].is_object());
        assert_eq!(res["result"]["serverInfo"]["name"], "coppice");
    }

    #[tokio::test]
    async fn initialize_unknown_version_falls_back() {
        let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        assert_eq!(res["result"]["protocolVersion"], "2025-06-18");
    }

    #[tokio::test]
    async fn initialize_legacy_version_falls_back() {
        let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        assert_eq!(res["result"]["protocolVersion"], "2025-06-18");
    }

    #[tokio::test]
    async fn notification_returns_none() {
        let body = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(handle_rpc(body, &FakeHost).await.is_none());
    }

    #[tokio::test]
    async fn tools_list_serializes_annotations() {
        let body = json!({"jsonrpc":"2.0","id":"a","method":"tools/list"});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        let tool = &res["result"]["tools"][0];
        assert_eq!(tool["name"], "look");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    #[tokio::test]
    async fn tools_call_wraps_output() {
        let body = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"look"}});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        assert_eq!(res["result"]["content"][0]["type"], "text");
        assert_eq!(res["result"]["content"][0]["text"], "hi");
        assert_eq!(res["result"]["isError"], true);
    }

    #[tokio::test]
    async fn unknown_method_is_32601() {
        let body = json!({"jsonrpc":"2.0","id":3,"method":"nope"});
        let res = handle_rpc(body, &FakeHost).await.unwrap();
        assert_eq!(res["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn batch_is_32600() {
        let res = handle_rpc(json!([]), &FakeHost).await.unwrap();
        assert_eq!(res["error"]["code"], -32600);
        assert!(res["id"].is_null());
    }
}
