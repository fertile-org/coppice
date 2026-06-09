use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_client::OpenCodeClient;
use crate::sessions::opencode_events::coppice_run_prompt;
use crate::sessions::opencode_serve::OpenCodeServeManager;
use async_trait::async_trait;
use coppice_config::OpenCodeProviderConfig;
use std::path::PathBuf;
use std::sync::Arc;

pub struct OpenCodeProvider {
    serve: Arc<OpenCodeServeManager>,
    #[allow(dead_code)]
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
        let context_path = PathBuf::from(&input.context_path);
        let worktree = context_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let client = OpenCodeClient::new(self.serve.base_url());
        client
            .run_session(
                worktree,
                input.model_provider.as_deref(),
                input.model.as_deref(),
                coppice_run_prompt(),
                input.stream,
                input.cancel_rx,
                input.session_created_tx,
            )
            .await
    }
}
