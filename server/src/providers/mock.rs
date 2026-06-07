use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError, fixtures_root};
use async_trait::async_trait;
use std::path::PathBuf;

pub struct MockProvider {
    fixtures_dir: PathBuf,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self {
            fixtures_dir: fixtures_root(),
        }
    }
}

impl MockProvider {
    pub fn new(fixtures_dir: PathBuf) -> Self {
        Self { fixtures_dir }
    }

    fn response_name(&self) -> String {
        std::env::var("MOCK_AGENT_RESPONSE").unwrap_or_else(|_| "done".into())
    }
}

#[async_trait]
impl AgentProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn run(&self, _input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let path = self.fixtures_dir.join(format!("{}.json", self.response_name()));
        let raw = std::fs::read_to_string(&path).map_err(|_| {
            ProviderError::FixtureNotFound(path.display().to_string())
        })?;
        serde_json::from_str(&raw).map_err(|err| ProviderError::InvalidFixture(err.to_string()))
    }
}
