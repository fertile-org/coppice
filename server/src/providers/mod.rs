pub mod mock;
pub mod opencode;
pub mod opencode_models;
pub mod registry;

pub use registry::ConnectorRegistry;

/// Temporary alias for gradual migration.
pub type ProviderRegistry = ConnectorRegistry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::watch;

use crate::sessions::run_registry::RunStreamHandle;

#[derive(Clone)]
pub struct AgentRunInput {
    pub agent_id: String,
    pub ticket_id: Option<String>,
    pub context_path: String,
    pub run_id: Option<String>,
    pub artifacts_dir: Option<String>,
    pub stream: Option<Arc<RunStreamHandle>>,
    pub cancel_rx: Option<watch::Receiver<bool>>,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub session_created_tx: Option<watch::Sender<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRunResult {
    Done {
        summary: String,
        #[serde(default, rename = "changedFiles")]
        changed_files: Vec<String>,
        #[serde(default, rename = "testsRun")]
        tests_run: Vec<String>,
        #[serde(default, rename = "nextStatus")]
        next_status: Option<String>,
        #[serde(default, rename = "assignTo")]
        assign_to: Option<String>,
        #[serde(default, rename = "mentionAgents")]
        mention_agents: Vec<String>,
        #[serde(default)]
        blockers: Vec<String>,
    },
    Blocked {
        #[serde(rename = "blockerType")]
        blocker_type: String,
        summary: String,
        #[serde(default, rename = "nextStatus")]
        next_status: Option<String>,
        #[serde(default, rename = "assignTo")]
        assign_to: Option<String>,
        #[serde(rename = "mentionAgents")]
        mention_agents: Vec<String>,
        #[serde(default, rename = "requiredCapabilities")]
        required_capabilities: Vec<String>,
        #[serde(default, rename = "requiredSecrets")]
        required_secrets: Vec<String>,
    },
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("fixture not found: {0}")]
    FixtureNotFound(String),
    #[error("invalid fixture: {0}")]
    InvalidFixture(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("run cancelled")]
    Cancelled,
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError>;
}

pub fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/agent-responses")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;

    #[test]
    fn agent_run_result_deserializes_done_fixture() {
        let path = fixtures_root().join("done.json");
        let raw = std::fs::read_to_string(path).expect("read done fixture");
        let result: AgentRunResult = serde_json::from_str(&raw).expect("deserialize done");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }
    }

    #[tokio::test]
    async fn mock_provider_returns_done_fixture() {
        let provider = MockProvider::default();
        let result = provider
            .run(AgentRunInput {
                agent_id: "agent-1".into(),
                ticket_id: None,
                context_path: "/tmp".into(),
                run_id: None,
                artifacts_dir: None,
                stream: None,
                cancel_rx: None,
                model_provider: None,
                model: None,
                session_created_tx: None,
            })
            .await
            .expect("mock run");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }
    }
}
