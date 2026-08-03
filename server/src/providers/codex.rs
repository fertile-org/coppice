use super::codex_console::CodexConsolePublisher;
use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use async_trait::async_trait;
use coppice_config::CodexProviderConfig;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

pub struct CodexProvider {
    config: CodexProviderConfig,
}

impl CodexProvider {
    pub fn new(config: CodexProviderConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentProvider for CodexProvider {
    fn id(&self) -> &str {
        "codex"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let context_path = PathBuf::from(&input.context_path);
        let worktree = context_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);

        // Build the codex exec command.
        // Codex CLI uses `codex exec` for non-interactive mode with `--json` for structured output.
        // The prompt is passed via stdin since codex exec reads from stdin when no prompt arg is given.
        let mut cmd = Command::new("codex");
        cmd.arg("exec")
            .arg("--json")
            .arg("--dangerously-bypass-approvals-and-sandbox")
            .arg("-C")
            .arg(worktree)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(model) = &input.model {
            cmd.arg("-m").arg(model);
        }

        // Resume: if we have a session_id, use the resume subcommand.
        // Note: Codex session resume is documented as unreliable. This may not work
        // reliably until the Codex CLI stabilizes this feature.
        let resume = if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                Some(sid.clone())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(sid) = &resume {
            cmd.arg("resume").arg(sid);
        }

        // Auth is host-managed: the operator runs `codex login` wherever the server runs.
        // The child process inherits that environment directly — same model as claude-code
        // and opencode. Coppice does not inject or strip credentials.

        let mut child = cmd
            .spawn()
            .map_err(ProviderError::Io)?;

        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");

        // Write the prompt to stdin.
        let prompt = coppice_run_prompt();
        let stdin_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });

        let mut reader = BufReader::new(stdout).lines();
        let deadline = tokio::time::Instant::now() + run_timeout;
        let mut cancel_rx = input.cancel_rx;

        // Pump stderr to tracing so we don't lose diagnostics.
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "codex.stderr", "{line}");
            }
        });

        let mut assistant_text = String::new();
        let mut session_sent = false;
        let mut console = CodexConsolePublisher::new();

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
                        "codex run timed out after {}s",
                        run_timeout.as_secs()
                    )));
                }

                line = reader.next_line() => {
                    match line {
                        Ok(Some(raw)) => {
                            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                                continue;
                            };

                            // Capture thread_id early for the job worker.
                            // Codex uses "thread_id" instead of "session_id".
                            if !session_sent {
                                if let Some(sid) = value
                                    .get("thread_id")
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
                                console.handle_json(stream, &value);
                            }

                            // Accumulate assistant text.
                            if let Some(text) = extract_assistant_text(&value) {
                                assistant_text.push_str(&text);
                            }

                            // Terminal event — turn.completed indicates the run is finished.
                            if value.get("type").and_then(|v| v.as_str()) == Some("turn.completed") {
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
        let _ = stdin_task.await;
        let _ = stderr_task.await;

        if !status.success() {
            return Err(ProviderError::InvalidFixture(format!(
                "codex exited with status {status}"
            )));
        }

        extract_result_from_text(&assistant_text).ok_or_else(|| {
            ProviderError::InvalidFixture(
                "no result contract found in codex output".into(),
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
    // Codex CLI event format:
    // - {"type":"thread.started","thread_id":"..."}
    // - {"type":"turn.started"}
    // - {"type":"item.completed","item":{"type":"agent_message","text":"..."}}
    // - {"type":"turn.completed","usage":{...}}

    let ty = value.get("type").and_then(|v| v.as_str())?;
    if ty == "item.completed" {
        let item = value.get("item")?;
        item.get("id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.trim().is_empty())?;
        let item_type = item.get("type").and_then(|v| v.as_str())?;
        if item_type == "agent_message" {
            return item.get("text").and_then(|v| v.as_str()).map(str::to_string);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/codex")
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
        }
        let result = extract_result_from_text(&assistant_text).expect("extract result");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "Codex feature implementation complete.");
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
    fn extract_assistant_text_from_agent_message() {
        // Codex format: item.completed with agent_message
        let event = serde_json::json!({
            "type": "item.completed",
            "item": {
                "id": "item_0",
                "type": "agent_message",
                "text": "Codex is working"
            }
        });
        let text = extract_assistant_text(&event).expect("assistant text");
        assert_eq!(text, "Codex is working");
    }

    #[test]
    fn extract_assistant_text_ignores_command_execution() {
        // Codex format: item.completed with command_execution should be ignored
        let event = serde_json::json!({
            "type": "item.completed",
            "item": {
                "id": "item_1",
                "type": "command_execution",
                "command": "/bin/echo test",
                "exit_code": 0
            }
        });
        assert!(extract_assistant_text(&event).is_none());
    }

    #[test]
    fn extract_assistant_text_ignores_reasoning() {
        let event = serde_json::json!({
            "type": "item.completed",
            "item": {
                "id": "item_2",
                "type": "reasoning",
                "text": "{\"status\":\"done\",\"summary\":\"Not the final result.\"}"
            }
        });

        assert!(extract_assistant_text(&event).is_none());
    }

    #[test]
    fn extract_assistant_text_ignores_agent_message_without_id() {
        let event = serde_json::json!({
            "type": "item.completed",
            "item": {
                "type": "agent_message",
                "text": "{\"status\":\"done\",\"summary\":\"Malformed.\"}"
            }
        });

        assert!(extract_assistant_text(&event).is_none());
    }

    #[test]
    fn provider_id() {
        let provider = CodexProvider::new(CodexProviderConfig::default());
        assert_eq!(provider.id(), "codex");
    }

    #[test]
    fn thread_id_extracted_from_thread_started_event() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");
        let mut captured_id = None::<String>;
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(sid) = value
                .get("thread_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                captured_id = Some(sid.to_string());
                break;
            }
        }
        assert_eq!(captured_id.as_deref(), Some("codex_thread_abc123"));
    }
}
