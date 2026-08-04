use super::{fixtures_root, AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
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

    fn fixture_path(&self, input: &AgentRunInput) -> PathBuf {
        if let Ok(override_name) = std::env::var("MOCK_AGENT_RESPONSE") {
            return self.fixtures_dir.join(format!("{override_name}.json"));
        }
        let resume = self.fixtures_dir.join(&input.agent_key).join("resume.json");
        if input.job_type == "work_on_ticket"
            && input
                .resume_context
                .as_deref()
                .is_some_and(has_resume_signal)
            && resume.exists()
        {
            return resume;
        }
        let ready = self.fixtures_dir.join(&input.agent_key).join("ready.json");
        if input.job_type == "work_on_ticket"
            && is_ready_ticket_context(&input.context_path)
            && ready.exists()
        {
            return ready;
        }
        let keyed = self
            .fixtures_dir
            .join(&input.agent_key)
            .join(format!("{}.json", input.job_type));
        if keyed.exists() {
            return keyed;
        }
        self.fixtures_dir
            .join(&input.agent_key)
            .join("default.json")
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

fn has_resume_signal(context: &str) -> bool {
    let context = context.to_ascii_lowercase();
    context.contains("prior blocker")
        || context.contains("last checkpoint")
        || context.contains("(blocked)")
        || context.contains("(clarification answer)")
        || context.contains("(progress update)")
}

fn is_ready_ticket_context(context_path: &str) -> bool {
    std::fs::read_to_string(context_path)
        .ok()
        .and_then(|context| {
            context
                .lines()
                .find_map(|line| line.trim().strip_prefix("**Status:**"))
                .map(|status| status.trim().eq_ignore_ascii_case("ready"))
        })
        .unwrap_or(false)
}

#[async_trait]
impl AgentProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        if let (Some(stream), Some(mut cancel_rx)) = (&input.stream, input.cancel_rx.clone()) {
            crate::sessions::scripted_stream::emit_script(
                stream,
                &mut cancel_rx,
                crate::sessions::scripted_stream::MOCK_SCRIPT,
            )
            .await;
            if *cancel_rx.borrow() {
                return Err(ProviderError::Cancelled);
            }
        }

        let path = self.fixture_path(&input);
        let raw = std::fs::read_to_string(&path)
            .map_err(|_| ProviderError::FixtureNotFound(path.display().to_string()))?;
        let result: AgentRunResult = serde_json::from_str(&raw)
            .map_err(|err| ProviderError::InvalidFixture(err.to_string()))?;
        Self::maybe_write_stdout(&input)?;
        Ok(result)
    }
}

#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

#[cfg(test)]
static MOCK_ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn mock_env_lock() -> MutexGuard<'static, ()> {
    MOCK_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn env_lock() -> MutexGuard<'static, ()> {
        mock_env_lock()
    }

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

        fn clear(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::remove_var(key);
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

    fn base_input(agent_key: &str, job_type: &str) -> AgentRunInput {
        AgentRunInput {
            agent_id: agent_key.into(),
            agent_key: agent_key.into(),
            job_type: job_type.into(),
            ticket_id: None,
            context_path: "/tmp".into(),
            run_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
        }
    }

    #[tokio::test]
    async fn resolves_fixture_by_agent_key_and_job_type() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::clear("MOCK_AGENT_RESPONSE");
        let provider = MockProvider::new(fixtures_root());

        let pm_done = provider
            .run(base_input("pm", "work_on_ticket"))
            .await
            .expect("pm work_on_ticket");
        match pm_done {
            AgentRunResult::Done {
                assign_to,
                agent_requests,
                summary,
                ..
            } => {
                assert_eq!(
                    summary,
                    "Refined ticket scope and prepared it for Tech Lead technical refinement."
                );
                assert_eq!(assign_to.as_deref(), Some("tech_lead"));
                assert!(agent_requests.is_empty());
            }
            _ => panic!("expected done variant"),
        }

        let engineer_blocked = provider
            .run(base_input("backend_engineer", "work_on_ticket"))
            .await
            .expect("backend_engineer work_on_ticket");
        match engineer_blocked {
            AgentRunResult::Blocked {
                mention_agents,
                summary,
                ..
            } => {
                assert!(summary.contains("option A or B"));
                assert_eq!(mention_agents, vec!["pm".to_string()]);
            }
            _ => panic!("expected blocked variant"),
        }

        let mut ordinary_thread_input = base_input("backend_engineer", "work_on_ticket");
        ordinary_thread_input.resume_context =
            Some("Recent activity:\n- **PM Agent** (implementation done): Refined scope.".into());
        let ordinary_thread_result = provider
            .run(ordinary_thread_input)
            .await
            .expect("ordinary ticket thread");
        assert!(matches!(
            ordinary_thread_result,
            AgentRunResult::Blocked { .. }
        ));

        let pm_mention = provider
            .run(base_input("pm", "respond_to_mention"))
            .await
            .expect("pm respond_to_mention");
        match pm_mention {
            AgentRunResult::Done { summary, .. } => {
                assert!(summary.contains("option A"));
            }
            _ => panic!("expected done variant"),
        }

        let mut resume_input = base_input("backend_engineer", "work_on_ticket");
        resume_input.resume_context = Some("Prior blocker context".into());
        let engineer_resume = provider.run(resume_input).await.expect("engineer resume");
        match engineer_resume {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Implementation complete.");
            }
            _ => panic!("expected done variant"),
        }

        let continued_path = fixtures_root().join("backend_engineer/continued.json");
        let continued_raw = std::fs::read_to_string(&continued_path).expect("read continued");
        let continued: AgentRunResult =
            serde_json::from_str(&continued_raw).expect("parse continued");
        match continued {
            AgentRunResult::Continued {
                summary,
                progress_note,
                ..
            } => {
                assert!(summary.contains("TmuxStream"));
                assert!(progress_note.as_deref().unwrap().contains("tmux_stream.rs"));
            }
            _ => panic!("expected continued variant"),
        }

        let mut checkpoint_resume = base_input("backend_engineer", "work_on_ticket");
        checkpoint_resume.resume_context =
            Some("**Last checkpoint:** Implemented TmuxStream create/kill.".into());
        let second_run = provider
            .run(checkpoint_resume)
            .await
            .expect("second run after continued");
        match second_run {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Implementation complete.");
            }
            _ => panic!("expected done variant after checkpoint resume"),
        }
    }

    #[tokio::test]
    async fn tech_lead_ready_uses_refinement_fixture() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::clear("MOCK_AGENT_RESPONSE");
        let provider = MockProvider::new(fixtures_root());
        let contexts = TempDir::new().expect("context tempdir");
        let ready_context = contexts.path().join("ready.md");
        std::fs::write(
            &ready_context,
            "# Current task\n\n**Status:** ready\n\n# Ticket thread\n\n**Status:** in_review",
        )
        .expect("write Ready context");
        let review_context = contexts.path().join("review.md");
        std::fs::write(&review_context, "# Current task\n\n**Status:** in_review\n")
            .expect("write review context");

        let mut ready_input = base_input("tech_lead", "work_on_ticket");
        ready_input.context_path = ready_context.to_string_lossy().into_owned();
        let ready_result = provider
            .run(ready_input)
            .await
            .expect("Ready Tech Lead fixture");
        match ready_result {
            AgentRunResult::Done {
                assign_to,
                changed_files,
                summary,
                ..
            } => {
                assert_eq!(assign_to.as_deref(), Some("backend_engineer"));
                assert!(changed_files.is_empty());
                assert!(summary.contains("technical approach"));
            }
            _ => panic!("expected Ready refinement result"),
        }

        let mut review_input = base_input("tech_lead", "work_on_ticket");
        review_input.context_path = review_context.to_string_lossy().into_owned();
        let review_result = provider
            .run(review_input)
            .await
            .expect("In Review Tech Lead fixture");
        match review_result {
            AgentRunResult::Done {
                assign_to, summary, ..
            } => {
                assert!(assign_to.is_none());
                assert!(summary.contains("## Verdict"));
            }
            _ => panic!("expected review result"),
        }
    }

    #[tokio::test]
    async fn env_override_still_resolves_root_fixture() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "done");
        let provider = MockProvider::new(fixtures_root());
        let result = provider
            .run(base_input("pm", "work_on_ticket"))
            .await
            .expect("env override run");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Mock implementation complete.");
            }
            _ => panic!("expected done variant"),
        }
    }

    #[tokio::test]
    async fn writes_stdout_sidecar_when_env_set() {
        let _lock = env_lock();
        let _stdout_guard = EnvGuard::set("MOCK_AGENT_STDOUT", "1");
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "done");
        let artifacts = TempDir::new().expect("temp artifacts dir");
        let run_id = "test-run-id";

        let provider = MockProvider::default();
        let result = provider
            .run(AgentRunInput {
                agent_id: "agent-1".into(),
                agent_key: "agent-1".into(),
                job_type: "work_on_ticket".into(),
                ticket_id: None,
                context_path: "/tmp".into(),
                run_id: Some(run_id.into()),
                artifacts_dir: Some(artifacts.path().to_string_lossy().into_owned()),
                stream: None,
                cancel_rx: None,
                model_provider: None,
                model: None,
                session_created_tx: None,
                resume_context: None,
                resume_session_id: None,
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
