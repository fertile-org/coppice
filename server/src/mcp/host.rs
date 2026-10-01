use crate::mcp::protocol::{ToolDefinition, ToolHost, ToolResult};
use crate::mcp::token::RunToolScope;
use crate::AppState;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// One run token's view of the gateway; all routing lives in `AppState.tools`.
pub struct RunToolHost {
    state: Arc<AppState>,
    scope: RunToolScope,
}

impl RunToolHost {
    pub fn new(state: Arc<AppState>, scope: RunToolScope) -> Self {
        Self { state, scope }
    }
}

#[async_trait]
impl ToolHost for RunToolHost {
    async fn list(&self) -> Vec<ToolDefinition> {
        self.state
            .tools
            .tools_for(&self.scope)
            .await
            .into_iter()
            .map(|tool| tool.def)
            .collect()
    }

    async fn call(&self, name: &str, args: Value) -> ToolResult {
        self.state
            .tools
            .call(&self.state, &self.scope, name, args)
            .await
    }
}
