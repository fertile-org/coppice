//! `McpConnection` over an `rmcp` client service, shared by every rmcp-backed transport.

use super::transport::{McpConnection, ProxyError, RemoteTool};
use crate::mcp::protocol::{ToolContent, ToolResult, SERVER_NAME};
use async_trait::async_trait;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ClientConfig, ClientRequest,
    ContentBlock, Implementation, ListToolsRequest, PaginatedRequestParams, ProtocolVersion,
    ServerResult, Tool,
};
use rmcp::service::{NotificationContext, RoleClient, RunningService};
use rmcp::transport::IntoTransport;
use rmcp::{ClientHandler, Peer, ServiceError, ServiceExt};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_LIST_PAGES: usize = 100;
const REDACTED: &str = "[redacted]";
/// Shorter setting values are too likely to occur naturally in tool output.
const MIN_OUTPUT_SECRET_LEN: usize = 6;

/// Scrubs resolved secrets (env/header values, credentialed URLs) out of error text.
#[derive(Clone, Default)]
pub(super) struct Redactor {
    /// `(secret, replacement)`, longest secret first.
    rules: Vec<(String, String)>,
}

impl Redactor {
    /// Redacts setting and server-env values of any length; for error text.
    pub(super) fn for_errors(secrets: &[String]) -> Self {
        secrets
            .iter()
            .fold(Self::default(), |r, value| r.secret(value))
    }

    /// Redacts setting and server-env values long enough to be unambiguous; for tool output.
    pub(super) fn for_output(secrets: &[String]) -> Self {
        secrets
            .iter()
            .filter(|value| value.len() >= MIN_OUTPUT_SECRET_LEN)
            .fold(Self::default(), |r, value| r.secret(value))
    }

    pub(super) fn secret(self, value: &str) -> Self {
        self.replace(value, REDACTED)
    }

    pub(super) fn replace(mut self, value: &str, replacement: &str) -> Self {
        if !value.is_empty() && !self.rules.iter().any(|(s, _)| s == value) {
            self.rules
                .push((value.to_string(), replacement.to_string()));
            self.rules
                .sort_by_key(|(secret, _)| std::cmp::Reverse(secret.len()));
        }
        self
    }

    pub(super) fn apply(&self, message: &str) -> String {
        self.rules
            .iter()
            .fold(message.to_string(), |out, (secret, replacement)| {
                out.replace(secret, replacement)
            })
    }
}

#[derive(Clone)]
pub(super) struct Handler {
    tools_changed: Arc<AtomicBool>,
}

impl ClientHandler for Handler {
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        self.tools_changed.store(true, Ordering::SeqCst);
    }

    fn get_info(&self) -> ClientConfig {
        let mut info = ClientConfig::default();
        info.protocol_version = ProtocolVersion::LATEST_WITH_INITIALIZE;
        info.client_info = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"));
        info
    }
}

pub(super) struct RmcpConnection {
    peer: Peer<RoleClient>,
    service: Mutex<Option<RunningService<RoleClient, Handler>>>,
    tools_changed: Arc<AtomicBool>,
    closed: AtomicBool,
    redactor: Redactor,
    output: Redactor,
}

/// Runs the `initialize` handshake over `transport`. `redactor` scrubs error text,
/// `output` the text blocks of tool results.
pub(super) async fn connect<T, E, A>(
    transport: T,
    redactor: Redactor,
    output: Redactor,
) -> Result<Box<dyn McpConnection>, ProxyError>
where
    T: IntoTransport<RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let tools_changed = Arc::new(AtomicBool::new(false));
    let handler = Handler {
        tools_changed: tools_changed.clone(),
    };
    let service = handler
        .serve(transport)
        .await
        .map_err(|e| ProxyError::Start(redactor.apply(&e.to_string())))?;
    Ok(Box::new(RmcpConnection {
        peer: service.peer().clone(),
        service: Mutex::new(Some(service)),
        tools_changed,
        closed: AtomicBool::new(false),
        redactor,
        output,
    }))
}

impl RmcpConnection {
    fn map_err(&self, err: ServiceError) -> ProxyError {
        match err {
            ServiceError::TransportClosed | ServiceError::Cancelled { .. } => {
                self.closed.store(true, Ordering::SeqCst);
                ProxyError::Closed
            }
            ServiceError::Timeout { .. } => ProxyError::Timeout,
            ServiceError::McpError(data) => {
                ProxyError::Protocol(self.redactor.apply(&data.message))
            }
            other => ProxyError::Protocol(self.redactor.apply(&other.to_string())),
        }
    }
}

#[async_trait]
impl McpConnection for RmcpConnection {
    async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError> {
        let mut tools = Vec::new();
        let mut cursor = None;
        for _ in 0..MAX_LIST_PAGES {
            let request = ClientRequest::ListToolsRequest(ListToolsRequest {
                method: Default::default(),
                params: Some(PaginatedRequestParams::default().with_cursor(cursor)),
                extensions: Default::default(),
            });
            let ServerResult::ListToolsResult(page) = self
                .peer
                .send_request(request)
                .await
                .map_err(|e| self.map_err(e))?
            else {
                return Err(ProxyError::Protocol(
                    "unexpected response to tools/list".into(),
                ));
            };
            tools.extend(page.tools.into_iter().map(remote_tool));
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(tools);
            }
        }
        Err(ProxyError::Protocol("too many tools/list pages".into()))
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult, ProxyError> {
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = match args {
            Value::Object(map) => Some(map),
            Value::Null => None,
            _ => {
                return Err(ProxyError::Protocol(
                    "tool arguments must be a JSON object".into(),
                ))
            }
        };
        match self
            .peer
            .call_tool_once(params)
            .await
            .map_err(|e| self.map_err(e))?
        {
            CallToolResponse::Complete(result) => Ok(tool_result(result, &self.output)),
            _ => Err(ProxyError::Protocol(
                "unsupported tools/call response".into(),
            )),
        }
    }

    fn take_tools_changed(&self) -> bool {
        self.tools_changed.swap(false, Ordering::SeqCst)
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst) || self.peer.is_transport_closed()
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(mut service) = self.service.lock().await.take() {
            let _ = service.close_with_timeout(CLOSE_TIMEOUT).await;
        }
    }
}

fn remote_tool(tool: Tool) -> RemoteTool {
    RemoteTool {
        read_only: tool
            .annotations
            .as_ref()
            .and_then(|a| a.read_only_hint)
            .unwrap_or(false),
        name: tool.name.into_owned(),
        description: tool.description.map(|d| d.into_owned()).unwrap_or_default(),
        input_schema: Value::Object((*tool.input_schema).clone()),
    }
}

/// Text of both normal and `isError` results is redacted: error text is persisted
/// in `run_tool_calls.error` and shown in the UI.
fn tool_result(result: CallToolResult, redactor: &Redactor) -> ToolResult {
    ToolResult {
        content: result
            .content
            .into_iter()
            .map(|block| tool_content(block, redactor))
            .collect(),
        is_error: result.is_error.unwrap_or(false),
    }
}

fn tool_content(block: ContentBlock, redactor: &Redactor) -> ToolContent {
    let unsupported = |kind: &str| ToolContent::Text(format!("[unsupported content: {kind}]"));
    match block {
        ContentBlock::Text(text) => ToolContent::Text(redactor.apply(&text.text)),
        ContentBlock::Image(image) => ToolContent::Image {
            data: image.data,
            mime_type: image.mime_type,
        },
        ContentBlock::Audio(_) => unsupported("audio"),
        ContentBlock::Resource(_) => unsupported("resource"),
        ContentBlock::ResourceLink(_) => unsupported("resource_link"),
        _ => unsupported("unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_replaces_longest_first() {
        let redactor = Redactor::default()
            .secret("tok")
            .replace("http://u:tok@h/p?q=1", "http://h/p")
            .secret("");
        assert_eq!(
            redactor.apply("GET http://u:tok@h/p?q=1 failed, secret tok"),
            "GET http://h/p failed, secret [redacted]"
        );
    }

    #[test]
    fn content_mapping() {
        let json = serde_json::json!({
            "content": [
                { "type": "text", "text": "hi" },
                { "type": "image", "data": "AAA", "mimeType": "image/png" },
                { "type": "audio", "data": "AAA", "mimeType": "audio/wav" },
            ],
            "isError": true,
        });
        let result: CallToolResult = serde_json::from_value(json).unwrap();
        let mapped = tool_result(result, &Redactor::default());
        assert!(mapped.is_error);
        assert_eq!(
            mapped.content,
            vec![
                ToolContent::Text("hi".into()),
                ToolContent::Image {
                    data: "AAA".into(),
                    mime_type: "image/png".into()
                },
                ToolContent::Text("[unsupported content: audio]".into()),
            ]
        );
    }

    #[test]
    fn tool_result_text_is_redacted_for_ok_and_error() {
        let secrets = vec!["s3cr3t-value".to_string(), "abc".to_string()];
        let redactor = Redactor::for_output(&secrets);
        for is_error in [false, true] {
            let json = serde_json::json!({
                "content": [
                    { "type": "text", "text": "token=s3cr3t-value abc" },
                    { "type": "image", "data": "AAA", "mimeType": "image/png" },
                ],
                "isError": is_error,
            });
            let result: CallToolResult = serde_json::from_value(json).unwrap();
            let mapped = tool_result(result, &redactor);
            assert_eq!(mapped.is_error, is_error);
            assert_eq!(
                mapped.content,
                vec![
                    ToolContent::Text("token=[redacted] abc".into()),
                    ToolContent::Image {
                        data: "AAA".into(),
                        mime_type: "image/png".into()
                    },
                ]
            );
        }
        assert_eq!(
            Redactor::for_errors(&secrets).apply("token=s3cr3t-value abc"),
            "token=[redacted] [redacted]"
        );
    }

    #[test]
    fn read_only_from_annotations() {
        let tool: Tool = serde_json::from_value(serde_json::json!({
            "name": "t",
            "inputSchema": { "type": "object" },
            "annotations": { "readOnlyHint": true },
        }))
        .unwrap();
        let remote = remote_tool(tool);
        assert!(remote.read_only);
        assert_eq!(remote.description, "");
        assert_eq!(remote.input_schema, serde_json::json!({ "type": "object" }));
    }
}
