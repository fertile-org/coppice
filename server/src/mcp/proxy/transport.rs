//! Transport seam for plugin MCP servers. A new transport is one `McpTransport`
//! implementation plus one `Transports::register` call.

use super::{HttpTransport, StdioTransport};
use crate::mcp::protocol::ToolResult;
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub read_only: bool,
}

/// Messages never carry resolved env values, header values, or credentialed URLs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProxyError {
    #[error("MCP server failed to start: {0}")]
    Start(String),
    #[error("MCP protocol error: {0}")]
    Protocol(String),
    #[error("MCP connection closed")]
    Closed,
    #[error("MCP request timed out")]
    Timeout,
}

#[async_trait]
pub trait McpConnection: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError>;
    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult, ProxyError>;
    /// Returns whether the server announced a tool-list change since the last call.
    fn take_tools_changed(&self) -> bool;
    fn is_closed(&self) -> bool;
    async fn close(&self);
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    fn kind(&self) -> &'static str;
    /// Starts or opens the server and completes the MCP `initialize` handshake.
    async fn connect(
        &self,
        spec: &ResolvedTransport,
        cwd: &Path,
    ) -> Result<Box<dyn McpConnection>, ProxyError>;
}

#[derive(Clone, Default)]
pub struct Transports {
    by_kind: HashMap<&'static str, Arc<dyn McpTransport>>,
}

impl Transports {
    pub fn builtin() -> Self {
        let mut transports = Self::default();
        transports.register(Arc::new(StdioTransport));
        transports.register(Arc::new(HttpTransport));
        transports
    }

    pub fn register(&mut self, transport: Arc<dyn McpTransport>) {
        self.by_kind.insert(transport.kind(), transport);
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn McpTransport>> {
        self.by_kind.get(kind).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registers_stdio_and_http() {
        let transports = Transports::builtin();
        assert_eq!(transports.get("stdio").unwrap().kind(), "stdio");
        assert_eq!(transports.get("http").unwrap().kind(), "http");
        assert!(transports.get("sse").is_none());
    }
}
