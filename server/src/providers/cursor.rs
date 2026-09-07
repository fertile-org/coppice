use super::cursor_console::CursorConsolePublisher;
use super::{
    refuse_unsupported_read_only, worktree_dir_from_context, AgentProvider, AgentRunInput,
    AgentRunResult, ProviderError,
};
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use async_trait::async_trait;
use coppice_config::CursorProviderConfig;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

pub struct CursorProvider {
    config: CursorProviderConfig,
}

impl CursorProvider {
    pub fn new(config: CursorProviderConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentProvider for CursorProvider {
    fn id(&self) -> &str {
        "cursor"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        if input.read_only_tools {
            return Err(refuse_unsupported_read_only(self.id()));
        }
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        let command = self.config.command.as_str();

        let mut cmd = Command::new(command);
        cmd.arg("-p")
            .arg(coppice_run_prompt())
            .arg("--trust")
            .arg("--force")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--workspace")
            .arg(&worktree)
            .current_dir(&worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(model) = &input.model {
            cmd.arg("--model").arg(model);
        }

        // Resume a previous cursor session if we have its session_id.
        if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                cmd.arg("--resume").arg(sid);
            }
        }

        // Auth is host-managed: the operator runs `agent login` wherever the
        // server runs. The child process inherits that environment directly.
        // Coppice does not inject or strip credentials.

        tracing::info!(
            command,
            cwd = %worktree.display(),
            model = input.model.as_deref().unwrap_or(""),
            resume = input.resume_session_id.as_deref().unwrap_or(""),
            "starting cursor connector subprocess"
        );

        let mut child = cmd.spawn().map_err(|err| {
            ProviderError::Io(std::io::Error::new(
                err.kind(),
                format!(
                    "failed to spawn `{command}` (cwd {}): {err}",
                    worktree.display()
                ),
            ))
        })?;

        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let mut reader = BufReader::new(stdout).lines();
        let deadline = tokio::time::Instant::now() + run_timeout;
        let mut cancel_rx = input.cancel_rx;

        // Collect stderr for failure messages; also mirror to tracing.
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "cursor.stderr", "{line}");
                if lines.len() < 40 {
                    lines.push(line);
                }
            }
            lines
        });

        let mut assistant_text = String::new();
        let mut session_sent = false;
        let mut result_error: Option<String> = None;
        let mut saw_result_event = false;
        let mut console = CursorConsolePublisher::new();

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
                    let stderr_tail = stderr_tail_from_task(stderr_task).await;
                    return Err(ProviderError::InvalidFixture(format!(
                        "`{command}` timed out after {}s{}",
                        run_timeout.as_secs(),
                        format_stderr_suffix(&stderr_tail)
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

                            // Terminal result event.
                            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                                saw_result_event = true;
                                if result_event_is_error(&value) {
                                    result_error = Some(
                                        value
                                            .get("result")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("cursor run failed")
                                            .to_string(),
                                    );
                                    break;
                                }
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
        let stderr_tail = stderr_tail_from_task(stderr_task).await;

        if let Some(msg) = result_error {
            return Err(ProviderError::InvalidFixture(format!(
                "`{command}` result error: {msg}{}",
                format_stderr_suffix(&stderr_tail)
            )));
        }

        // Prefer a successful stream result over a weird non-zero exit.
        if !status.success() && !saw_result_event {
            return Err(ProviderError::InvalidFixture(format!(
                "`{command}` exited with {status} (cwd {}){}",
                worktree.display(),
                format_stderr_suffix(&stderr_tail)
            )));
        }
        if !status.success() && saw_result_event {
            tracing::warn!(
                command,
                %status,
                stderr = %stderr_tail.join("\n"),
                "cursor CLI returned a result event but exited non-zero; accepting stream result"
            );
        }

        extract_result_from_text(&assistant_text).ok_or_else(|| {
            ProviderError::InvalidFixture(format!(
                "no result contract found in `{command}` output{}",
                format_stderr_suffix(&stderr_tail)
            ))
        })
    }
}

async fn stderr_tail_from_task(
    stderr_task: tokio::task::JoinHandle<Vec<String>>,
) -> Vec<String> {
    stderr_task.await.unwrap_or_default()
}

fn format_stderr_suffix(lines: &[String]) -> String {
    let joined = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if joined.is_empty() {
        String::new()
    } else {
        format!("; stderr: {joined}")
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
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn result_event_is_error(value: &serde_json::Value) -> bool {
    if value.get("type").and_then(|v| v.as_str()) != Some("result") {
        return false;
    }
    if value.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        return true;
    }
    matches!(value.get("subtype").and_then(|v| v.as_str()), Some("error"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/cursor")
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
            if result_event_is_error(&value) {
                panic!("unexpected error result in done fixture");
            }
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
        let provider = CursorProvider::new(CursorProviderConfig::default());
        assert_eq!(provider.id(), "cursor");
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
        assert_eq!(captured_id.as_deref(), Some("sess_cursor_abc"));
    }

    #[test]
    fn error_result_is_rejected() {
        let raw = std::fs::read_to_string(fixtures_root().join("error.jsonl"))
            .expect("read error.jsonl");
        let mut saw_error = false;
        let mut assistant_text = String::new();
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if result_event_is_error(&value) {
                saw_error = true;
                continue;
            }
            if let Some(text) = extract_assistant_text(&value) {
                assistant_text.push_str(&text);
            }
            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                    assistant_text = final_text.to_string();
                }
            }
        }
        assert!(saw_error, "expected result_event_is_error on error fixture");
        assert!(
            extract_result_from_text(&assistant_text).is_none(),
            "error result must not be accepted as a success contract"
        );
    }

    fn publish_fixture_lines(
        handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>,
        raw: &str,
    ) {
        let mut console = CursorConsolePublisher::new();
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
            "session + 2 text + result"
        );
        assert_eq!(events[0]["type"], "cursor.console.session");
        assert_eq!(events[1]["type"], "cursor.console.text");
        assert!(events[1]["markdown"]
            .as_str()
            .unwrap()
            .contains("Reading .agent/context.md"));
        assert_eq!(events[3]["type"], "cursor.console.result");
        assert_eq!(events[3]["contract"]["summary"], "Implemented the feature.");
    }
}
