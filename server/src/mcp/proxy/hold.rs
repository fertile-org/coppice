//! Holds write-time secret registrations for the life of an MCP connection.

use super::transport::{McpConnection, ProxyError, RemoteTool};
use crate::logs::SecretGuard;
use crate::mcp::protocol::ToolResult;
use async_trait::async_trait;
use serde_json::Value;

pub(super) struct MaskedConnection {
    pub(super) inner: Box<dyn McpConnection>,
    pub(super) _secrets: SecretGuard,
}

#[async_trait]
impl McpConnection for MaskedConnection {
    async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError> {
        self.inner.list_tools().await
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult, ProxyError> {
        self.inner.call_tool(name, args).await
    }

    fn take_tools_changed(&self) -> bool {
        self.inner.take_tools_changed()
    }

    fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    async fn close(&self) {
        self.inner.close().await;
    }
}
