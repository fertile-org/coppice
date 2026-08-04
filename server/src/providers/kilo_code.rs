use super::kilo_console::KiloConsolePublisher;
use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use async_trait::async_trait;
use coppice_config::KiloCodeProviderConfig;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

/// Kilo Code CLI connector.
///
/// Kilo is a documented OpenCode fork (`@kilocode/cli`, binary `kilo`). Public
/// docs describe `kilo run --format json` for machine-readable events but do
/// **not** document a stable HTTP/SSE daemon API or confirm OpenCode endpoint
/// compatibility, so this connector uses the subprocess path (like `codex` /
/// `claude-code`) rather than the OpenCode daemon client.
///
/// The exact `--format json` event schema for the installed Kilo version is not
/// verified in CI. Event parsing is therefore defensive: session IDs and
/// assistant text are pulled from the common OpenCode-style shapes plus generic
/// top-level `text` / `content` fields. Result extraction relies on
/// `extract_result_from_text`, which scans the accumulated assistant output for
/// the JSON result contract regardless of the surrounding event envelope.
pub struct KiloCodeProvider {
    config: KiloCodeProviderConfig,
}

impl KiloCodeProvider {
    pub fn new(config: KiloCodeProviderConfig) -> Self {
        Self { config }
    }

    /// Build the `--model` argument. Kilo expects `provider/model` format.
    /// When both are configured, join them. When only `model` is set, pass it
    /// through (allows a fully-qualified `provider/model` string stored in
    /// `model`). When only `model_provider` is set, omit the flag.
    fn model_arg(&self, input: &AgentRunInput) -> Option<String> {
        match (&input.model_provider, &input.model) {
            (Some(provider), Some(model)) => {
                if model.contains('/') {
                    Some(model.clone())
                } else {
                    Some(format!("{provider}/{model}"))
                }
            }
            (None, Some(model)) => Some(model.clone()),
            _ => None,
        }
    }
}

#[async_trait]
impl AgentProvider for KiloCodeProvider {
    fn id(&self) -> &str {
        "kilo-code"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let context_path = PathBuf::from(&input.context_path);
        let worktree = context_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);

        // `kilo run` accepts the message as a positional arg. `--format json`
        // emits raw JSON events on stdout. `--auto` auto-approves permissions
        // for non-interactive / pipeline usage. There is no documented `-C`
        // working-directory flag on `kilo run`, so we set the process CWD.
        let mut cmd = Command::new(&self.config.command);
        cmd.arg("run")
            .arg("--format")
            .arg("json")
            .arg("--auto")
            .arg(coppice_run_prompt())
            .current_dir(worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(model) = self.model_arg(&input) {
            cmd.arg("--model").arg(model);
        }

        // Resume a previous Kilo session if we have its id. `kilo run -s <id>`
        // is documented for resuming a specific session.
        if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                cmd.arg("--session").arg(sid);
            }
        }

        // Auth is host-managed: the operator runs `kilo` → `/connect` (or
        // `kilo auth login`) wherever the server runs. The child process
        // inherits that environment directly — same model as claude-code and
        // codex. Coppice does not inject or strip credentials.

        let mut child = cmd.spawn().map_err(ProviderError::Io)?;

        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");

        let mut reader = BufReader::new(stdout).lines();
        let deadline = tokio::time::Instant::now() + run_timeout;
        let mut cancel_rx = input.cancel_rx;

        // Pump stderr to tracing so we don't lose diagnostics.
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "kilo_code.stderr", "{line}");
            }
        });

        let mut assistant_text = String::new();
        let mut session_sent = false;
        let mut console = KiloConsolePublisher::new();

        loop {
            if is_cancelled(&cancel_rx) {
                let _ = child.kill().await;
                return Err(ProviderError::Cancelled);
            }

            tokio::select! {
                biased;

                _ = wait_cancel(&mut cancel_rx) => {
                    if is_cancelled(&cancel_rx) {
                        let _ = child.kill().await;
                        return Err(ProviderError::Cancelled);
                    }
                }

                _ = tokio::time::sleep_until(deadline) => {
                    let _ = child.kill().await;
                    return Err(ProviderError::InvalidFixture(format!(
                        "kilo-code run timed out after {}s",
                        run_timeout.as_secs()
                    )));
                }

                line = reader.next_line() => {
                    match line {
                        Ok(Some(raw)) => {
                            let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                                continue;
                            };

                            // Capture session id early for the job worker.
                            if !session_sent {
                                if let Some(sid) = extract_session_id(&value) {
                                    if let Some(tx) = &input.session_created_tx {
                                        let _ = tx.send(sid);
                                    }
                                    session_sent = true;
                                }
                            }

                            // Forward structured console events to the run stream.
                            if let Some(stream) = &input.stream {
                                console.handle_json(stream, &value);
                            }

                            // Accumulate assistant text.
                            if let Some(text) = extract_assistant_text(&value) {
                                assistant_text.push_str(&text);
                            }

                            // Terminal event. Kilo/OpenCode use session.idle /
                            // session.finished to signal end of turn. We also break
                            // on stdout EOF (Ok(None)) below as a backstop.
                            if is_terminal_event(&value) {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = child.kill().await;
                            return Err(ProviderError::Io(e));
                        }
                    }
                }
            }
        }

        // Wait for the process to exit.
        let status = child.wait().await.map_err(ProviderError::Io)?;
        let _ = stderr_task.await;

        if !status.success() {
            return Err(ProviderError::InvalidFixture(format!(
                "kilo-code exited with status {status}"
            )));
        }

        extract_result_from_text(&assistant_text).ok_or_else(|| {
            ProviderError::InvalidFixture(
                "no result contract found in kilo-code output".into(),
            )
        })
    }
}

fn is_cancelled(cancel_rx: &Option<watch::Receiver<bool>>) -> bool {
    cancel_rx.as_ref().is_some_and(|rx| *rx.borrow())
}

async fn wait_cancel(cancel_rx: &mut Option<watch::Receiver<bool>>) {
    match cancel_rx {
        Some(rx) => {
            let _ = rx.changed().await;
        }
        None => std::future::pending::<()>().await,
    }
}

fn is_terminal_event(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(|v| v.as_str()),
        Some("session.idle") | Some("session.finished")
    )
}

/// Extract a session id from common OpenCode-style event shapes. Defensive:
/// returns the first non-empty id-like field found.
fn extract_session_id(value: &Value) -> Option<String> {
    let candidates = [
        value
            .get("properties")
            .and_then(|p| p.get("sessionID"))
            .and_then(|v| v.as_str()),
        value.get("sessionID").and_then(|v| v.as_str()),
        value.get("session_id").and_then(|v| v.as_str()),
        value
            .get("session")
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str()),
        value
            .get("properties")
            .and_then(|p| p.get("session"))
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str()),
    ];
    for c in candidates {
        if let Some(s) = c.filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Extract assistant text from common event shapes. Handles OpenCode-style
/// `session.message` events (assistant role with text parts) and falls back to
/// top-level `text` / `content` fields for simpler envelopes.
pub(crate) fn extract_assistant_text(value: &Value) -> Option<String> {
    let ty = value.get("type").and_then(|v| v.as_str())?;
    if ty == "session.message" {
        let message = value.get("properties")?.get("message")?;
        let role = message
            .get("info")
            .and_then(|i| i.get("role"))
            .or_else(|| message.get("role"))
            .and_then(|r| r.as_str());
        if role != Some("assistant") {
            return None;
        }
        let parts = message.get("parts")?.as_array()?;
        let mut text = String::new();
        for part in parts {
            let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if part_type != "text" && part_type != "reasoning" && part_type != "compaction" {
                continue;
            }
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                text.push_str(t);
            }
        }
        return if text.is_empty() { None } else { Some(text) };
    }
    // Generic fallback: top-level text/content. Tool events don't carry these,
    // so this does not pick up tool output.
    value
        .get("text")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("content").and_then(|v| v.as_str()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/kilo-code")
    }

    #[test]
    fn provider_id() {
        let provider = KiloCodeProvider::new(KiloCodeProviderConfig::default());
        assert_eq!(provider.id(), "kilo-code");
    }

    #[test]
    fn model_arg_joins_provider_and_model() {
        let provider = KiloCodeProvider::new(KiloCodeProviderConfig::default());
        let input = AgentRunInput {
            agent_id: "a".into(),
            agent_key: "a".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_path: "/tmp/.agent/context.md".into(),
            run_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-20250514".into()),
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
        };
        assert_eq!(
            provider.model_arg(&input).as_deref(),
            Some("anthropic/claude-sonnet-4-20250514")
        );
    }

    #[test]
    fn model_arg_passes_through_qualified_model() {
        let provider = KiloCodeProvider::new(KiloCodeProviderConfig::default());
        let input = AgentRunInput {
            agent_id: "a".into(),
            agent_key: "a".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_path: "/tmp/.agent/context.md".into(),
            run_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: Some("anthropic".into()),
            model: Some("anthropic/claude-sonnet-4-20250514".into()),
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
        };
        assert_eq!(
            provider.model_arg(&input).as_deref(),
            Some("anthropic/claude-sonnet-4-20250514")
        );
    }

    #[test]
    fn model_arg_none_when_only_provider() {
        let provider = KiloCodeProvider::new(KiloCodeProviderConfig::default());
        let input = AgentRunInput {
            agent_id: "a".into(),
            agent_key: "a".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_path: "/tmp/.agent/context.md".into(),
            run_id: None,
            artifacts_dir: None,
            stream: None,
            cancel_rx: None,
            model_provider: Some("anthropic".into()),
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
        };
        assert!(provider.model_arg(&input).is_none());
    }

    #[test]
    fn extract_session_id_from_properties() {
        let event = serde_json::json!({
            "type": "session.updated",
            "properties": {"sessionID": "kilo_sess_123"}
        });
        assert_eq!(
            extract_session_id(&event).as_deref(),
            Some("kilo_sess_123")
        );
    }

    #[test]
    fn extract_session_id_from_nested_session() {
        let event = serde_json::json!({
            "type": "session.idle",
            "properties": {"session": {"id": "kilo_sess_456"}}
        });
        assert_eq!(
            extract_session_id(&event).as_deref(),
            Some("kilo_sess_456")
        );
    }

    #[test]
    fn extract_assistant_text_from_session_message() {
        let event = serde_json::json!({
            "type": "session.message",
            "properties": {
                "message": {
                    "info": {"role": "assistant"},
                    "parts": [
                        {"type": "text", "text": "Reading .agent/context.md..."},
                        {"type": "text", "text": "Implementing..."}
                    ]
                }
            }
        });
        let text = extract_assistant_text(&event).expect("assistant text");
        assert!(text.contains("Reading .agent/context.md"));
        assert!(text.contains("Implementing"));
    }

    #[test]
    fn extract_assistant_text_ignores_user_message() {
        let event = serde_json::json!({
            "type": "session.message",
            "properties": {
                "message": {
                    "info": {"role": "user"},
                    "parts": [{"type": "text", "text": "go"}]
                }
            }
        });
        assert!(extract_assistant_text(&event).is_none());
    }

    #[test]
    fn extract_result_from_stream_json_done_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");
        let mut assistant_text = String::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(text) = extract_assistant_text(&value) {
                assistant_text.push_str(&text);
            }
        }
        let result = extract_result_from_text(&assistant_text).expect("extract result");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Kilo feature implementation complete.");
            }
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn extract_result_from_stream_json_blocked_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("blocked.jsonl"))
            .expect("read blocked.jsonl");
        let mut assistant_text = String::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(text) = extract_assistant_text(&value) {
                assistant_text.push_str(&text);
            }
        }
        let result = extract_result_from_text(&assistant_text).expect("extract result");
        match result {
            AgentRunResult::Blocked { blocker_type, .. } => {
                assert_eq!(blocker_type, "missing_capability");
            }
            _ => panic!("expected blocked"),
        }
    }

    #[test]
    fn session_id_extracted_from_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");
        let mut captured_id = None::<String>;
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(sid) = extract_session_id(&value) {
                captured_id = Some(sid);
                break;
            }
        }
        assert_eq!(captured_id.as_deref(), Some("kilo_sess_abc123"));
    }

    fn collect_console_events(
        messages: &[crate::sessions::LiveMessage],
    ) -> Vec<serde_json::Value> {
        messages
            .iter()
            .filter_map(|msg| match msg {
                crate::sessions::LiveMessage::Event { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    fn publish_fixture_lines(
        handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>,
        raw: &str,
    ) {
        let mut console = KiloConsolePublisher::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            console.handle_json(handle, &value);
        }
    }

    #[test]
    fn streaming_pipeline_publishes_console_events() {
        use crate::sessions::run_registry::RunStreamRegistry;

        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let events = collect_console_events(&handle.buffered_tail());
        assert_eq!(events.len(), 2, "assistant text + result");
        assert_eq!(events[0]["type"], "kilo.console.text");
        assert!(events[0]["markdown"]
            .as_str()
            .unwrap()
            .contains("Reading .agent/context.md"));
        assert_eq!(events[1]["type"], "kilo.console.result");
        assert_eq!(events[1]["contract"]["summary"], "Kilo feature implementation complete.");
    }

    #[test]
    fn streaming_pipeline_blocked_fixture_publishes_console_events() {
        use crate::sessions::run_registry::RunStreamRegistry;

        let raw = std::fs::read_to_string(fixtures_root().join("blocked.jsonl"))
            .expect("read blocked.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let events = collect_console_events(&handle.buffered_tail());
        assert_eq!(events.len(), 2, "assistant text + blocked result");
        assert_eq!(events.last().unwrap()["contract"]["status"], "blocked");
    }
}
