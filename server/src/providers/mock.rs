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

    fn maybe_write_stdout(input: &AgentRunInput) -> Result<(), ProviderError> {
        if std::env::var("MOCK_AGENT_STDOUT").as_deref() != Ok("1") {
            return Ok(());
        }
        let (Some(artifacts_dir), Some(run_id)) = (&input.artifacts_dir, &input.run_id) else {
            return Ok(());
        };
        // Sidecar file for M04 live-console prep; no DB artifact row in M03.
        let stdout_path = PathBuf::from(artifacts_dir)
            .join("runs")
            .join(run_id)
            .join("stdout.log");
        if let Some(parent) = stdout_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &stdout_path,
            "Mock agent starting...\nRunning tests...\nDone.\n",
        )?;
        Ok(())
    }
}

#[async_trait]
impl AgentProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let path = self.fixtures_dir.join(format!("{}.json", self.response_name()));
        let raw = std::fs::read_to_string(&path).map_err(|_| {
            ProviderError::FixtureNotFound(path.display().to_string())
        })?;
        let result: AgentRunResult = serde_json::from_str(&raw)
            .map_err(|err| ProviderError::InvalidFixture(err.to_string()))?;
        Self::maybe_write_stdout(&input)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[tokio::test]
    async fn writes_stdout_sidecar_when_env_set() {
        let _stdout_guard = EnvGuard::set("MOCK_AGENT_STDOUT", "1");
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "done");
        let artifacts = TempDir::new().expect("temp artifacts dir");
        let run_id = "test-run-id";

        let provider = MockProvider::default();
        let result = provider
            .run(AgentRunInput {
                agent_id: "agent-1".into(),
                ticket_id: None,
                context_path: "/tmp".into(),
                run_id: Some(run_id.into()),
                artifacts_dir: Some(artifacts.path().to_string_lossy().into_owned()),
            })
            .await
            .expect("mock run");

        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }

        let stdout_path = artifacts
            .path()
            .join("runs")
            .join(run_id)
            .join("stdout.log");
        assert!(stdout_path.exists());
        let content = std::fs::read_to_string(stdout_path).expect("read stdout");
        assert!(content.contains("Mock agent starting"));
        assert!(content.contains("Done."));
    }
}
