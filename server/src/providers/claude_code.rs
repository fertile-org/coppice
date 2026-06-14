use super::claude_console::ClaudeConsolePublisher;
use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use async_trait::async_trait;
use coppice_config::ClaudeCodeProviderConfig;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

const ALLOWED_TOOLS: &str = "Read,Write,Edit,MultiEdit,Bash,NotebookEdit,WebFetch,WebSearch,Glob,Grep,TodoWrite,Task";

pub struct ClaudeCodeProvider {
    config: ClaudeCodeProviderConfig,
}

impl ClaudeCodeProvider {
    pub fn new(config: ClaudeCodeProviderConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentProvider for ClaudeCodeProvider {
    fn id(&self) -> &str {
        "claude-code"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let context_path = PathBuf::from(&input.context_path);
        let worktree = context_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(coppice_run_prompt())
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--allowedTools")
            .arg(ALLOWED_TOOLS)
            .arg("--permission-mode")
            .arg("bypassPermissions")
            .current_dir(worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(model) = &input.model {
            cmd.arg("--model").arg(model);
        }

        // Resume a previous claude-code session if we have its session_id.
        if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                cmd.arg("--resume").arg(sid);
            }
        }

        // Auth is host-managed: the operator runs `claude login` (or sets
        // ANTHROPIC_API_KEY) wherever the server runs. The child process
        // inherits that environment directly — same model as the opencode
        // connector. Coppice does not inject or strip credentials.

        let mut child = cmd
            .spawn()
            .map_err(ProviderError::Io)?;

        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");

        let mut reader = BufReader::new(stdout).lines();
        let deadline = tokio::time::Instant::now() + run_timeout;
        let mut cancel_rx = input.cancel_rx;

        // Pump stderr to tracing so we don't lose diagnostics.
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "claude_code.stderr", "{line}");
            }
        });

        let mut assistant_text = String::new();
        let mut session_sent = false;
        let mut console = ClaudeConsolePublisher::new();

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
                        "claude-code run timed out after {}s",
                        run_timeout.as_secs()
                    )));
                }

                line = reader.next_line() => {
                    match line {
                        Ok(Some(raw)) => {
                            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                                continue;
                            };

                            // Capture session_id early for the job worker.
                            if !session_sent {
                                if let Some(sid) = value
                                    .get("session_id")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                {
                                    if let Some(tx) = &input.session_created_tx {
                                        let _ = tx.send(sid.to_string());
                                    }
                                    session_sent = true;
                                }
                            }

                            // Forward structured console events to the run stream.
                            if let Some(stream) = &input.stream {
                                console.handle_stream_json(stream, &value);
                            }

                            // Accumulate assistant text.
                            if let Some(text) = extract_assistant_text(&value) {
                                assistant_text.push_str(&text);
                            }

                            // Terminal result event — extract final text.
                            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                                if let Some(final_text) =
                                    value.get("result").and_then(|v| v.as_str())
                                {
                                    assistant_text = final_text.to_string();
                                }
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
                "claude-code exited with status {status}"
            )));
        }

        extract_result_from_text(&assistant_text).ok_or_else(|| {
            ProviderError::InvalidFixture(
                "no result contract found in claude-code output".into(),
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

fn extract_assistant_text(value: &serde_json::Value) -> Option<String> {
    let ty = value.get("type").and_then(|v| v.as_str())?;
    if ty != "assistant" {
        return None;
    }
    let content = value.get("message")?.get("content")?.as_array()?;
    let mut text = String::new();
    for part in content {
        if part.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                text.push_str(t);
            }
        }
    }
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/claude-code")
    }

    #[test]
    fn extract_result_from_stream_json_done_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");
        let mut assistant_text = String::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(text) = extract_assistant_text(&value) {
                assistant_text.push_str(&text);
            }
            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                    assistant_text = final_text.to_string();
                }
            }
        }
        let result = extract_result_from_text(&assistant_text).expect("extract result");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Implemented the feature.");
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
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(text) = extract_assistant_text(&value) {
                assistant_text.push_str(&text);
            }
            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                    assistant_text = final_text.to_string();
                }
            }
        }
        let result = extract_result_from_text(&assistant_text).expect("extract result");
        match result {
            AgentRunResult::Blocked { blocker_type, .. } => {
                assert_eq!(blocker_type, "missing_secret");
            }
            _ => panic!("expected blocked"),
        }
    }

    #[test]
    fn provider_id() {
        let provider = ClaudeCodeProvider::new(ClaudeCodeProviderConfig::default());
        assert_eq!(provider.id(), "claude-code");
    }

    #[test]
    fn session_id_extracted_from_init_event() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");
        let mut session_sent = false;
        let mut captured_id = None::<String>;
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if !session_sent {
                if let Some(sid) = value
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    captured_id = Some(sid.to_string());
                    session_sent = true;
                }
            }
        }
        assert_eq!(captured_id.as_deref(), Some("sess_abc123"));
    }

    fn publish_fixture_lines(
        handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>,
        raw: &str,
    ) {
        let mut console = crate::providers::claude_console::ClaudeConsolePublisher::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            console.handle_stream_json(handle, &value);
        }
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

    #[test]
    fn streaming_pipeline_publishes_console_events() {
        use crate::sessions::run_registry::RunStreamRegistry;

        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let events = collect_console_events(&handle.buffered_tail());
        assert_eq!(
            events.len(),
            4,
            "session + 2 text + result; duplicate result event is skipped"
        );
        assert_eq!(events[0]["type"], "claude.console.session");
        assert_eq!(events[1]["type"], "claude.console.text");
        assert!(events[1]["markdown"]
            .as_str()
            .unwrap()
            .contains("Reading .agent/context.md"));
        assert_eq!(events[3]["type"], "claude.console.result");
        assert_eq!(events[3]["contract"]["summary"], "Implemented the feature.");
    }

    #[test]
    fn streaming_pipeline_agentic_fixture_includes_tool_activity() {
        use crate::sessions::run_registry::RunStreamRegistry;

        let raw = std::fs::read_to_string(fixtures_root().join("agentic.jsonl"))
            .expect("read agentic.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let events = collect_console_events(&handle.buffered_tail());
        let tool_titles: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "claude.console.tool")
            .filter_map(|e| e["title"].as_str())
            .collect();
        assert!(tool_titles.iter().any(|t| t.contains("codex.rs")));
        assert!(tool_titles.iter().any(|t| t.contains("cargo test")));

        let has_result = events.iter().any(|e| e["type"] == "claude.console.result");
        assert!(has_result);
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
        assert_eq!(events.len(), 3, "session + text + blocked result");
        assert_eq!(events.last().unwrap()["contract"]["status"], "blocked");
    }

    #[test]
    fn buffered_tail_replays_console_events_for_recovery() {
        use crate::sessions::run_registry::RunStreamRegistry;
        use crate::sessions::LiveMessage;

        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let tail = handle.buffered_tail();
        assert_eq!(tail.len(), 4);
        assert!(tail
            .iter()
            .all(|m| matches!(m, LiveMessage::Event { .. })));
    }
}
