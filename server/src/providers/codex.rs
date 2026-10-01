use super::cli_runner::{
    run_cli_with_stdin, CliError, CliInvocation, LineHandler, LineStep, RunIo,
};
use super::codex_console::CodexConsolePublisher;
use super::{
    worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult, ProviderError,
};
use crate::mcp::grant::McpAccess;
use crate::mcp::wiring::McpServerSpec;
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use crate::sessions::run_registry::RunStreamHandle;
use async_trait::async_trait;
use coppice_config::CodexProviderConfig;
use std::sync::Arc;
use std::time::Duration;

/// `-c` overrides that add the gateway as the `coppice` MCP server for this
/// process only. The token is read by the CLI from `COPPICE_MCP_TOKEN`, so it
/// never lands in argv or in `~/.codex/config.toml`.
///
/// Unverified against a live CLI — see `docs/providers/README.md`.
fn codex_mcp_args(access: &McpAccess) -> Vec<String> {
    McpServerSpec::from_access(access).codex_args()
}

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
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);

        // Build the codex exec command.
        // Codex CLI uses `codex exec` for non-interactive mode with `--json` for structured output.
        // The prompt is passed via stdin since codex exec reads from stdin when no prompt arg is given.
        let mut args = vec![
            "exec".to_string(),
            "--json".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "-C".to_string(),
            worktree.display().to_string(),
        ];

        if let Some(model) = &input.model {
            args.push("-m".to_string());
            args.push(model.clone());
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

        let mut env = Vec::new();
        // Before `resume`, so the overrides bind to `exec` and not to the subcommand.
        if let Some(access) = &input.mcp {
            args.extend(codex_mcp_args(access));
            env.extend(access.env().map(|(k, v)| (k.to_string(), v)));
        }

        if let Some(sid) = &resume {
            args.push("resume".to_string());
            args.push(sid.clone());
        }

        // Auth is host-managed: the operator runs `codex login` wherever the server runs.
        // The child process inherits that environment directly — same model as claude-code
        // and opencode. Coppice does not inject or strip credentials.

        let invocation = CliInvocation {
            program: "codex".to_string(),
            args,
            env,
            // Codex takes its root from `-C`; the process keeps the server's cwd.
            cwd: std::env::current_dir().map_err(ProviderError::Io)?,
            timeout: run_timeout,
        };
        let mut handler = CodexLines {
            stream: input.stream.clone(),
            console: CodexConsolePublisher::new(),
            assistant_text: String::new(),
        };
        let io = RunIo {
            cancel_rx: input.cancel_rx,
            session_created_tx: input.session_created_tx,
            stderr_target: "codex.stderr",
        };

        let exit = match run_cli_with_stdin(
            invocation,
            Some(coppice_run_prompt().to_string()),
            &mut handler,
            io,
        )
        .await
        {
            Ok(exit) => exit,
            Err(CliError::Spawn(e) | CliError::Io(e)) => return Err(ProviderError::Io(e)),
            Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
            Err(CliError::TimedOut { .. }) => {
                return Err(ProviderError::InvalidFixture(format!(
                    "codex run timed out after {}s",
                    run_timeout.as_secs()
                )));
            }
        };

        if !exit.status.success() {
            return Err(ProviderError::InvalidFixture(format!(
                "codex exited with status {}",
                exit.status
            )));
        }

        extract_result_from_text(&handler.assistant_text).ok_or_else(|| {
            ProviderError::MissingResult("no result contract found in codex output".into())
        })
    }
}

/// Forwards console events and accumulates agent messages; `turn.completed`
/// is terminal. Codex reports its session as `thread_id`.
struct CodexLines {
    stream: Option<Arc<RunStreamHandle>>,
    console: CodexConsolePublisher,
    assistant_text: String,
}

impl LineHandler for CodexLines {
    fn on_json(&mut self, value: &serde_json::Value) -> LineStep {
        let session_id = value
            .get("thread_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(stream) = &self.stream {
            self.console.handle_json(stream, value);
        }

        if let Some(text) = extract_assistant_text(value) {
            self.assistant_text.push_str(&text);
        }

        LineStep {
            session_id,
            stop: value.get("type").and_then(|v| v.as_str()) == Some("turn.completed"),
        }
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
            return item
                .get("text")
                .and_then(|v| v.as_str())
                .map(str::to_string);
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
        let raw =
            std::fs::read_to_string(fixtures_root().join("done.jsonl")).expect("read done.jsonl");
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
    fn codex_mcp_args_use_env_bearer() {
        let access = crate::mcp::grant::McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "super-secret-run-token".into(),
        };
        let args = codex_mcp_args(&access);
        assert_eq!(args, McpServerSpec::from_access(&access).codex_args());
        assert!(!args.join(" ").contains("super-secret-run-token"));
    }

    #[test]
    fn thread_id_extracted_from_thread_started_event() {
        let raw =
            std::fs::read_to_string(fixtures_root().join("done.jsonl")).expect("read done.jsonl");
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
