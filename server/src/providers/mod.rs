pub mod mock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRunInput {
    pub agent_id: String,
    pub ticket_id: Option<String>,
    pub context_path: String,
    pub run_id: Option<String>,
    pub artifacts_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRunResult {
    Done {
        summary: String,
        #[serde(rename = "changedFiles")]
        changed_files: Vec<String>,
        #[serde(rename = "testsRun")]
        tests_run: Vec<String>,
        #[serde(rename = "nextStatus")]
        next_status: String,
        #[serde(rename = "mentionAgents")]
        mention_agents: Vec<String>,
        blockers: Vec<String>,
    },
    Blocked {
        #[serde(rename = "blockerType")]
        blocker_type: String,
        summary: String,
        #[serde(rename = "nextStatus")]
        next_status: String,
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
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
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
