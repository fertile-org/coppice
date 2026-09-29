use super::{
    mcp_unavailable, worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult,
    ProviderError,
};
use crate::mcp::grant::McpAccess;
use crate::sessions::opencode_client::OpenCodeClient;
use crate::sessions::opencode_events::coppice_run_prompt;
use crate::sessions::opencode_serve::OpenCodeServeManager;
use async_trait::async_trait;
use coppice_config::OpenCodeProviderConfig;
use std::sync::Arc;
use std::time::Duration;

pub struct OpenCodeProvider {
    serve: Arc<OpenCodeServeManager>,
    config: OpenCodeProviderConfig,
}

impl OpenCodeProvider {
    pub fn new(serve: Arc<OpenCodeServeManager>, config: OpenCodeProviderConfig) -> Self {
        Self { serve, config }
    }
}

#[async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str {
        "opencode"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        if let Some(access) = &input.mcp {
            opencode_mcp_setup(access)?;
        }
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        let client = OpenCodeClient::with_run_timeout(self.serve.base_url(), run_timeout);
        client
            .run_session(
                &worktree,
                input.model_provider.as_deref(),
                input.model.as_deref(),
                input.resume_session_id.as_deref(),
                coppice_run_prompt(),
                input.stream,
                input.cancel_rx,
                input.session_created_tx,
            )
            .await
    }
}

/// The shared `opencode serve` process is started once for the whole server, so
/// it cannot carry a per-run MCP server or a per-run bearer token. Rather than
/// leak one run's token to every session, refuse the run.
fn opencode_mcp_setup(_access: &McpAccess) -> Result<(), ProviderError> {
    Err(mcp_unavailable("opencode has no per-run MCP configuration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_mcp_setup_refuses_tool_first_runs() {
        let access = McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "super-secret-run-token".into(),
        };
        let err = opencode_mcp_setup(&access).expect_err("opencode cannot be tool-first");
        assert!(matches!(err, ProviderError::InvalidInput(_)));
        assert_eq!(
            err.to_string(),
            "invalid input: mcp_unavailable: opencode has no per-run MCP configuration"
        );
    }
}
