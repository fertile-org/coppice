use super::{fixtures_root, AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::is_ready_tech_lead_refinement;
use async_trait::async_trait;
use serde::Deserialize;
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
        if let Some(override_name) = std::env::var("MOCK_AGENT_RESPONSE")
            .ok()
            .filter(|name| !name.is_empty())
        {
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
        if input.ticket_status.is_some_and(|status| {
            is_ready_tech_lead_refinement(
                input.context_profile,
                &input.job_type,
                status_to_str(status),
                &input.agent_key,
                &input.agent_role,
            )
        }) && ready.exists()
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

    /// Tool-call results go to the same sidecar as the rest of the mock output.
    fn append_tool_output(input: &AgentRunInput, outputs: &[String]) -> Result<(), ProviderError> {
        if outputs.is_empty() || std::env::var("MOCK_AGENT_STDOUT").as_deref() != Ok("1") {
            return Ok(());
        }
        let (Some(artifacts_dir), Some(run_id)) = (&input.artifacts_dir, &input.run_id) else {
            return Ok(());
        };
        let stdout_path = PathBuf::from(artifacts_dir)
            .join("runs")
            .join(run_id)
            .join("stdout.log");
        if let Some(parent) = stdout_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stdout_path)?;
        for text in outputs {
            use std::io::Write;
            writeln!(file, "tool result: {text}")?;
        }
        Ok(())
    }
}

/// Compaction fixtures cannot know batch ids ahead of time: replace
/// `{{ticket:N}}` / `{{board:N}}` with the Nth ticket listed in the context,
/// and `{{ticket:last}}` / `{{board:last}}` with the newest one.
fn fill_compaction_placeholders(raw: &str, context_path: &str) -> String {
    let context = std::fs::read_to_string(context_path).unwrap_or_default();
    let listed = |prefix: &str| -> Vec<String> {
        context
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .filter_map(|rest| rest.split('`').next())
            .map(str::to_string)
            .collect()
    };
    let mut filled = raw.to_string();
    for (kind, prefix) in [("ticket", "- id: `"), ("board", "- board: `")] {
        let ids = listed(prefix);
        if let Some(last) = ids.last() {
            filled = filled.replace(&format!("{{{{{kind}:last}}}}"), last);
        }
        for (index, id) in ids.iter().enumerate() {
            filled = filled.replace(&format!("{{{{{kind}:{index}}}}}"), id);
        }
    }
    filled
}

fn has_resume_signal(context: &str) -> bool {
    let context = context.to_ascii_lowercase();
    context.contains("prior blocker")
        || context.contains("last checkpoint")
        || context.contains("(blocked)")
        || context.contains("(clarification answer)")
        || context.contains("(progress update)")
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

        if input.job_type == "chat_turn" {
            if let Some(tx) = &input.session_created_tx {
                let sid = input
                    .resume_session_id
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "mock-chat-session".to_string());
                let _ = tx.send(sid);
            }
            if std::env::var("MOCK_CHAT_RESUME_FAIL").as_deref() == Ok("1")
                && input
                    .resume_session_id
                    .as_ref()
                    .is_some_and(|s| !s.is_empty())
            {
                return Err(ProviderError::ResumeSessionInvalid(
                    "mock forced resume failure".into(),
                ));
            }
            if std::env::var("MOCK_CHAT_EXPECT_SLIM").as_deref() == Ok("1") {
                let body = std::fs::read_to_string(&input.context_path).map_err(ProviderError::Io)?;
                if body.contains("# Conversation transcript") {
                    return Err(ProviderError::InvalidFixture(
                        "expected slim resume context".into(),
                    ));
                }
            }
        }

        let path = self.fixture_path(&input);
        let raw = std::fs::read_to_string(&path)
            .map_err(|_| ProviderError::FixtureNotFound(path.display().to_string()))?;
        let raw = if input.job_type == crate::domain::knowledge_compaction::JOB_TYPE_COMPACT_KNOWLEDGE {
            fill_compaction_placeholders(&raw, &input.context_path)
        } else {
            raw
        };
        let mut value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|err| ProviderError::InvalidFixture(err.to_string()))?;
        let (tool_calls, delay_ms) = take_tool_directives(&mut value)?;
        let result: AgentRunResult = serde_json::from_value(value)
            .map_err(|err| ProviderError::InvalidFixture(err.to_string()))?;
        Self::maybe_write_stdout(&input)?;
        let outputs = run_tool_calls(&input, &tool_calls).await?;
        Self::append_tool_output(&input, &outputs)?;
        wait_after_tool_calls(&input, delay_ms).await?;
        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
struct MockToolCall {
    tool: String,
    #[serde(default = "empty_args")]
    args: serde_json::Value,
}

fn empty_args() -> serde_json::Value {
    serde_json::json!({})
}

/// Pull the mock-only `toolCalls` / `delayMsAfterToolCalls` keys out of the fixture.
fn take_tool_directives(
    value: &mut serde_json::Value,
) -> Result<(Vec<MockToolCall>, Option<u64>), ProviderError> {
    let Some(object) = value.as_object_mut() else {
        return Ok((Vec::new(), None));
    };
    let calls = match object.remove("toolCalls") {
        Some(raw) => serde_json::from_value(raw)
            .map_err(|err| ProviderError::InvalidFixture(format!("toolCalls: {err}")))?,
        None => Vec::new(),
    };
    let delay = match object.remove("delayMsAfterToolCalls") {
        Some(raw) => Some(raw.as_u64().ok_or_else(|| {
            ProviderError::InvalidFixture("delayMsAfterToolCalls must be a u64".into())
        })?),
        None => None,
    };
    Ok((calls, delay))
}

fn mcp_unavailable(err: impl std::fmt::Display) -> ProviderError {
    ProviderError::InvalidInput(format!("mcp_unavailable: {err}"))
}

/// Play the fixture's tool calls against the gateway: `initialize`, then one
/// `tools/call` per entry, in order. Returns each call's text result.
async fn run_tool_calls(
    input: &AgentRunInput,
    calls: &[MockToolCall],
) -> Result<Vec<String>, ProviderError> {
    let Some(mcp) = input.mcp.as_ref().filter(|_| !calls.is_empty()) else {
        return Ok(Vec::new());
    };
    let client = reqwest::Client::new();
    let rpc = |id: u64, method: &str, params: serde_json::Value| {
        let request = client
            .post(&mcp.url)
            .bearer_auth(&mcp.token)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }));
        async move {
            let response = request.send().await.map_err(mcp_unavailable)?;
            if !response.status().is_success() {
                return Err(mcp_unavailable(format!("HTTP {}", response.status())));
            }
            response
                .json::<serde_json::Value>()
                .await
                .map_err(mcp_unavailable)
        }
    };

    rpc(
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "coppice-mock", "version": "0" },
        }),
    )
    .await?;

    let mut outputs = Vec::with_capacity(calls.len());
    for (index, call) in calls.iter().enumerate() {
        let response = rpc(
            index as u64 + 2,
            "tools/call",
            serde_json::json!({ "name": call.tool, "arguments": call.args }),
        )
        .await?;
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .or_else(|| response["error"]["message"].as_str())
            .unwrap_or_default()
            .to_string();
        outputs.push(text);
    }
    Ok(outputs)
}

/// Hold the run open (still cancellable) after its tool calls.
async fn wait_after_tool_calls(
    input: &AgentRunInput,
    delay_ms: Option<u64>,
) -> Result<(), ProviderError> {
    let Some(delay_ms) = delay_ms else {
        return Ok(());
    };
    let sleep = tokio::time::sleep(std::time::Duration::from_millis(delay_ms));
    tokio::pin!(sleep);
    let Some(mut cancel_rx) = input.cancel_rx.clone() else {
        sleep.await;
        return Ok(());
    };
    loop {
        if *cancel_rx.borrow() {
            return Err(ProviderError::Cancelled);
        }
        tokio::select! {
            () = &mut sleep => return Ok(()),
            changed = cancel_rx.changed() => {
                if changed.is_err() {
                    sleep.await;
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
use crate::domain::context_profile::ContextProfile;
#[cfg(test)]
use crate::domain::substatus::TicketStatus;
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
            agent_role: match agent_key {
                "tech_lead" => "Technical Lead",
                "pm" => "PM",
                _ => "Backend Engineer",
            }
            .into(),
            job_type: job_type.into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Full,
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
            read_only_tools: false,
            mcp: None,
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
            "# Ticket context\n\n**Description:**\n\n# Agent role\n\n**Status:** in_review\n\n**Status:** ready\n\n# Agent role\n\n# Ticket thread\n\n**Status:** in_review",
        )
        .expect("write Ready context");
        let review_context = contexts.path().join("review.md");
        std::fs::write(
            &review_context,
            "# Ticket context\n\n**Description:**\n\n# Agent role\n\n**Status:** ready\n\n**Status:** in_review\n\n# Agent role\n",
        )
        .expect("write review context");

        let mut ready_input = base_input("tech_lead", "work_on_ticket");
        ready_input.ticket_status = Some(TicketStatus::Ready);
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
        review_input.ticket_status = Some(TicketStatus::InReview);
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

        let mut human_agent_input = base_input("tech_lead", "work_on_ticket");
        human_agent_input.ticket_status = Some(TicketStatus::Ready);
        human_agent_input.context_profile = ContextProfile::HumanAgent;
        let human_agent_result = provider
            .run(human_agent_input)
            .await
            .expect("Human Agent Tech Lead fixture");
        match human_agent_result {
            AgentRunResult::Done {
                assign_to, summary, ..
            } => {
                assert!(assign_to.is_none());
                assert!(summary.contains("## Verdict"));
            }
            _ => panic!("expected regular work fixture for Human Agent mode"),
        }
    }

    #[tokio::test]
    async fn empty_env_override_uses_agent_fixture() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "");
        let provider = MockProvider::new(fixtures_root());

        let result = provider
            .run(base_input("pm", "work_on_ticket"))
            .await
            .expect("empty override falls back to agent fixture");
        match result {
            AgentRunResult::Done {
                assign_to, summary, ..
            } => {
                assert_eq!(assign_to.as_deref(), Some("tech_lead"));
                assert_eq!(
                    summary,
                    "Refined ticket scope and prepared it for Tech Lead technical refinement."
                );
            }
            _ => panic!("expected done variant"),
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
                agent_role: "Backend Engineer".into(),
                job_type: "work_on_ticket".into(),
                ticket_id: None,
                ticket_status: None,
                context_profile: ContextProfile::Full,
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
                        read_only_tools: false,
                        mcp: None,
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

    #[tokio::test]
    async fn tool_call_keys_are_stripped_and_skipped_without_mcp() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "fx");
        let dir = TempDir::new().expect("fixtures dir");
        std::fs::write(
            dir.path().join("fx.json"),
            r#"{"status":"done","summary":"ok","toolCalls":[{"tool":"board_agents"}],"delayMsAfterToolCalls":1}"#,
        )
        .expect("write fixture");
        let provider = MockProvider::new(dir.path().to_path_buf());
        let result = provider
            .run(base_input("pm", "work_on_ticket"))
            .await
            .expect("run");
        assert!(matches!(result, AgentRunResult::Done { .. }));
    }

    #[tokio::test]
    async fn unreachable_gateway_is_mcp_unavailable() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "fx");
        let dir = TempDir::new().expect("fixtures dir");
        std::fs::write(
            dir.path().join("fx.json"),
            r#"{"status":"done","summary":"ok","toolCalls":[{"tool":"board_agents","args":{}}]}"#,
        )
        .expect("write fixture");
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/mcp", closed.local_addr().unwrap());
        drop(closed);
        let mut input = base_input("pm", "work_on_ticket");
        input.mcp = Some(crate::mcp::grant::McpAccess {
            url,
            token: "t".into(),
        });
        let err = MockProvider::new(dir.path().to_path_buf())
            .run(input)
            .await
            .expect_err("gateway down");
        assert!(
            matches!(&err, ProviderError::InvalidInput(m) if m.starts_with("mcp_unavailable")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn delay_after_tool_calls_honours_cancel() {
        let _lock = env_lock();
        let _response_guard = EnvGuard::set("MOCK_AGENT_RESPONSE", "fx");
        let dir = TempDir::new().expect("fixtures dir");
        std::fs::write(
            dir.path().join("fx.json"),
            r#"{"status":"done","summary":"ok","delayMsAfterToolCalls":60000}"#,
        )
        .expect("write fixture");
        let (tx, rx) = tokio::sync::watch::channel(false);
        let mut input = base_input("pm", "work_on_ticket");
        input.cancel_rx = Some(rx);
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = tx.send(true);
            tx
        });
        let err = MockProvider::new(dir.path().to_path_buf())
            .run(input)
            .await
            .expect_err("cancelled");
        assert!(matches!(err, ProviderError::Cancelled), "{err:?}");
        let _ = canceller.await;
    }

    #[test]
    fn fills_compaction_placeholders_from_the_context_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = dir.path().join("context.md");
        std::fs::write(
            &context,
            "### A\n\n- id: `t-1`\n- board: `b-1` (Core)\n\n### B\n\n- id: `t-2`\n- board: `b-2` (Web)\n",
        )
        .expect("write context");
        let filled = fill_compaction_placeholders(
            "{{ticket:0}} {{board:0}} {{ticket:last}} {{board:last}} {{ticket:9}}",
            &context.to_string_lossy(),
        );
        assert_eq!(filled, "t-1 b-1 t-2 b-2 {{ticket:9}}");
    }
}
