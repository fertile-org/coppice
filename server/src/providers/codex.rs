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

const ALLOWED_TOOLS: &str = "Read,Write,Edit,MultiEdit,NotebookEdit,WebFetch,WebSearch,Glob,Grep,TodoWrite,Task";

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

        let mut cmd = Command::new("codex");
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

        // Resume a previous codex session if we have its session_id.
        // Note: session resume is documented as unreliable for codex.
        // This follows the same pattern as claude-code but may not work
        // reliably until the Codex CLI stabilizes this feature.
        if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                cmd.arg("--resume").arg(sid);
            }
        }

        // Auth is host-managed: the operator runs `codex login` (or sets
        // the appropriate environment variable) wherever the server runs.
        // The child process inherits that environment directly — same model
        // as claude-code and opencode. Coppice does not inject or strip credentials.

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
                tracing::debug!(target: "codex.stderr", "{line}");
            }
        });

        let mut assistant_text = String::new();
        let mut frame_seq: u64 = 0;
        let mut session_sent = false;

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

                            // Forward display text to the run stream.
                            if let Some(stream) = &input.stream {
                                if let Some(text) = extract_display_text(&value) {
                                    stream.publish_frame(frame_seq, format!("{text}\n").into_bytes());
                                    frame_seq += 1;
                                }
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

fn extract_display_text(value: &serde_json::Value) -> Option<String> {
    let ty = value.get("type").and_then(|v| v.as_str())?;
    match ty {
        "assistant" => {
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
        "system" => value
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_string),
        "result" => value
            .get("result")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        _ => None,
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
            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                    assistant_text = final_text.to_string();
                }
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
            if value.get("type").and_then(|v| v.as_str()) == Some("result") {
                if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                    assistant_text = final_text.to_string();
                }
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
    fn extract_display_text_from_assistant_event() {
        let event = serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "text", "text": "Codex is working"}
                ]
            }
        });
        let text = extract_display_text(&event).expect("display text");
        assert_eq!(text, "Codex is working");
    }

    #[test]
    fn extract_display_text_from_result_event() {
        let event = serde_json::json!({
            "type": "result",
            "result": "Codex final output"
        });
        let text = extract_display_text(&event).expect("display text");
        assert_eq!(text, "Codex final output");
    }

    #[test]
    fn extract_display_text_ignores_tool_events() {
        let event = serde_json::json!({
            "type": "tool",
            "name": "Read"
        });
        assert!(extract_display_text(&event).is_none());
    }

    #[test]
    fn provider_id() {
        let provider = CodexProvider::new(CodexProviderConfig::default());
        assert_eq!(provider.id(), "codex");
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
        assert_eq!(captured_id.as_deref(), Some("codex_sess_abc123"));
    }

    #[test]
    fn session_id_extracted_from_result_event() {
        let event = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "result": "final output",
            "session_id": "codex_sess_xyz789"
        });
        let sid = event
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        assert_eq!(sid, Some("codex_sess_xyz789"));
    }

    #[test]
    fn streaming_pipeline_publishes_frames_to_run_stream_handle() {
        use crate::sessions::run_registry::RunStreamRegistry;
        use crate::sessions::LiveMessage;

        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let run_id = uuid::Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        let mut frame_seq: u64 = 0;
        let mut session_sent = false;

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
                    let _ = sid;
                    session_sent = true;
                }
            }

            if let Some(text) = extract_display_text(&value) {
                handle.publish_frame(frame_seq, format!("{text}\n").into_bytes());
                frame_seq += 1;
            }
        }

        let mut received = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            received.push(msg);
        }

        assert!(session_sent, "session_id should have been captured");
        assert_eq!(received.len(), 4, "should have 4 frames: 3 assistant + 1 result");
        for (i, msg) in received.iter().enumerate() {
            match msg {
                LiveMessage::Frame { seq, data } => {
                    assert_eq!(*seq, i as u64, "frame seq should be sequential");
                    assert!(!data.is_empty(), "frame data should not be empty");
                }
                _ => panic!("expected Frame, got {msg:?}"),
            }
        }

        let first_data = match &received[0] {
            LiveMessage::Frame { data, .. } => data.clone(),
            _ => unreachable!(),
        };
        let first = std::str::from_utf8(&first_data).unwrap();
        assert!(first.contains("Reading .agent/context.md"));
    }

    #[test]
    fn streaming_pipeline_blocked_fixture_publishes_frames() {
        use crate::sessions::run_registry::RunStreamRegistry;
        use crate::sessions::LiveMessage;

        let raw = std::fs::read_to_string(fixtures_root().join("blocked.jsonl"))
            .expect("read blocked.jsonl");

        let registry = RunStreamRegistry::new();
        let run_id = uuid::Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        let mut frame_seq: u64 = 0;
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(text) = extract_display_text(&value) {
                handle.publish_frame(frame_seq, format!("{text}\n").into_bytes());
                frame_seq += 1;
            }
        }

        let mut received = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            received.push(msg);
        }

        assert_eq!(received.len(), 3, "3 frames: 2 assistant + 1 result");
        assert!(received.iter().all(|m| matches!(m, LiveMessage::Frame { .. })));
    }

    #[test]
    fn buffered_tail_replays_frames_for_recovery() {
        use crate::sessions::run_registry::RunStreamRegistry;
        use crate::sessions::LiveMessage;

        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl"))
            .expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let run_id = uuid::Uuid::new_v4();
        let handle = registry.register(run_id);

        let mut frame_seq: u64 = 0;
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(text) = extract_display_text(&value) {
                handle.publish_frame(frame_seq, format!("{text}\n").into_bytes());
                frame_seq += 1;
            }
        }

        let tail = handle.buffered_tail();
        assert_eq!(tail.len(), 4, "buffered tail should have all 4 frames");
        assert!(tail.iter().all(|m| matches!(m, LiveMessage::Frame { .. })));
    }
}
