use super::claude_console::ClaudeConsolePublisher;
use super::cli_runner::{
    log_launch, run_cli, run_log_dir, CliError, CliInvocation, LineHandler, LineStep, RunIo,
};
use super::cli_version;
use super::{
    run_dir, worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult,
    ProviderError, CHAT_READ_ONLY_TOOLS,
};
use crate::mcp::grant::McpAccess;
use crate::mcp::wiring::{claude_tool_pattern, McpServerSpec};
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use crate::sessions::run_registry::RunStreamHandle;
use async_trait::async_trait;
use coppice_config::ClaudeCodeProviderConfig;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const ALLOWED_TOOLS: &str =
    "Read,Write,Edit,MultiEdit,Bash,NotebookEdit,WebFetch,WebSearch,Glob,Grep,TodoWrite,Task";

/// Per-run MCP config file (never the worktree or the user's `~/.claude.json`)
/// plus the flags that make `claude` load it and nothing else.
///
/// Unverified against a live CLI — see `docs/providers/README.md`.
fn claude_mcp_args(access: &McpAccess, run_dir: &Path) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(run_dir)?;
    let path = run_dir.join("mcp.json");
    // The token stays in the environment; the file only interpolates it.
    std::fs::write(&path, McpServerSpec::from_access(access).claude_json())?;
    Ok(vec![
        "--mcp-config".to_string(),
        path.display().to_string(),
        "--strict-mcp-config".to_string(),
    ])
}

fn append_model_flag(args: &mut Vec<String>, model: Option<&str>) {
    crate::providers::model_flag::push_model_flag(args, "--model", model);
}

fn claude_allowed_tools(read_only_tools: bool, mcp: bool) -> String {
    let base = if read_only_tools {
        CHAT_READ_ONLY_TOOLS
    } else {
        ALLOWED_TOOLS
    };
    if mcp {
        format!("{base},{}", claude_tool_pattern())
    } else {
        base.to_string()
    }
}

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
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        let descriptor = coppice_connectors::get(coppice_connectors::CLAUDE_CODE)
            .expect("claude-code descriptor");
        let contract = descriptor.run_contract;
        let program = descriptor.binary.to_string();
        let version = cli_version::detect(&program).await;
        let worktree_arg = worktree.display().to_string();
        let planning = input.job_type == crate::domain::workflow::JOB_TYPE_PLAN_TICKET;
        let read_only_tools = input.read_only_tools || planning;
        let mut args = contract.with_prompt(
            contract.argv(&coppice_connectors::LaunchSubst {
                read_only: read_only_tools,
                plan: planning,
                worktree: &worktree_arg,
                hostname: "",
                port: "",
                version: cli_version::for_gate(&version),
            }),
            coppice_run_prompt(),
        );
        args.push("--allowedTools".to_string());
        args.push(claude_allowed_tools(read_only_tools, input.mcp.is_some()));

        append_model_flag(&mut args, input.model.as_deref());

        // Resume a previous claude-code session if we have its session_id.
        if let Some(sid) = &input.resume_session_id {
            if !sid.is_empty() {
                args.push("--resume".to_string());
                args.push(sid.clone());
            }
        }

        // Auth is host-managed: the operator runs `claude login` (or sets
        // ANTHROPIC_API_KEY) wherever the server runs. The child process
        // inherits that environment directly — same model as the opencode
        // connector. Coppice does not inject or strip credentials.

        let mut env = Vec::new();
        if let Some(access) = &input.mcp {
            let run_dir = run_dir(&input, "claude-code")?;
            args.extend(claude_mcp_args(access, &run_dir)?);
            env.extend(access.env().map(|(k, v)| (k.to_string(), v)));
        }

        log_launch(
            &program,
            &version,
            &args,
            run_log_dir(input.artifacts_dir.as_deref(), input.run_id.as_deref()).as_deref(),
        );
        let invocation = CliInvocation {
            program,
            args,
            env,
            cwd: if contract.process_cwd_is_worktree {
                worktree
            } else {
                std::env::current_dir().map_err(ProviderError::Io)?
            },
            timeout: run_timeout,
            run_id: input.run_id.clone(),
            artifacts_dir: input.artifacts_dir.clone(),
            result_before_exit: contract.result_before_exit,
            result_exit_grace: None,
        };
        let mut handler = ClaudeLines {
            stream: input.stream.clone(),
            console: ClaudeConsolePublisher::new(super::tool_name_style(self.id())),
            assistant_text: String::new(),
        };
        let io = RunIo {
            cancel_rx: input.cancel_rx,
            session_created_tx: input.session_created_tx,
            stderr_target: "claude_code.stderr",
        };

        let exit = match run_cli(invocation, &mut handler, io).await {
            Ok(exit) => exit,
            Err(CliError::Spawn(e) | CliError::Io(e)) => return Err(ProviderError::Io(e)),
            Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
            Err(CliError::TimedOut { .. }) => {
                return Err(ProviderError::InvalidFixture(format!(
                    "claude-code run timed out after {}s",
                    run_timeout.as_secs()
                )));
            }
        };

        if !exit.status.success() && !exit.had_terminal_result {
            return Err(ProviderError::InvalidFixture(format!(
                "claude-code exited with status {}",
                exit.status
            )));
        }

        extract_result_from_text(&handler.assistant_text).ok_or_else(|| {
            ProviderError::MissingResult("no result contract found in claude-code output".into())
        })
    }
}

/// Forwards console events and accumulates assistant text; the `result`
/// event is terminal and its `result` field replaces the accumulated text.
struct ClaudeLines {
    stream: Option<Arc<RunStreamHandle>>,
    console: ClaudeConsolePublisher,
    assistant_text: String,
}

impl LineHandler for ClaudeLines {
    fn on_json(&mut self, value: &serde_json::Value) -> LineStep {
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(stream) = &self.stream {
            self.console.handle_stream_json(stream, value);
        }

        if let Some(text) = extract_assistant_text(value) {
            self.assistant_text.push_str(&text);
        }

        let stop = value.get("type").and_then(|v| v.as_str()) == Some("result");
        if stop {
            if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                self.assistant_text = final_text.to_string();
            }
        }
        LineStep { session_id, stop }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::SERVER_NAME;
    use std::path::PathBuf;

    #[test]
    fn blank_model_omits_model_flag_from_launched_command() {
        let contract = coppice_connectors::get(coppice_connectors::CLAUDE_CODE)
            .expect("claude-code descriptor")
            .run_contract;
        let base = contract.with_prompt(
            contract.argv(&coppice_connectors::LaunchSubst {
                read_only: false,
                plan: false,
                worktree: "/tmp/wt",
                hostname: "",
                port: "",
                version: None,
            }),
            "prompt",
        );
        for model in [None, Some(""), Some("   ")] {
            let mut args = base.clone();
            append_model_flag(&mut args, model);
            assert!(
                args.iter().all(|arg| arg != "--model" && arg != "-m"),
                "model {model:?} launched {args:?}"
            );
        }
    }

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/claude-code")
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

    fn access() -> crate::mcp::grant::McpAccess {
        crate::mcp::grant::McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "super-secret-run-token".into(),
        }
    }

    #[test]
    fn claude_mcp_args_write_run_file_outside_worktree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let run_dir = tmp.path().join("runs").join("run-1");
        let args = claude_mcp_args(&access(), &run_dir).expect("mcp args");

        let path = run_dir.join("mcp.json");
        assert_eq!(
            args,
            vec![
                "--mcp-config".to_string(),
                path.display().to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );

        let raw = std::fs::read_to_string(&path).expect("read mcp.json");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("parse mcp.json");
        let server = &doc["mcpServers"][SERVER_NAME];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:5000/mcp");
        assert_eq!(
            server["headers"]["Authorization"],
            "Bearer ${COPPICE_MCP_TOKEN}"
        );
        assert!(!raw.contains("super-secret-run-token"));
    }

    #[test]
    fn claude_allowed_tools_add_gateway_tools_in_both_modes() {
        let ticket = claude_allowed_tools(false, true);
        assert!(ticket.starts_with(ALLOWED_TOOLS));
        assert!(ticket.ends_with(",mcp__coppice__*"));

        let chat = claude_allowed_tools(true, true);
        assert!(chat.starts_with(CHAT_READ_ONLY_TOOLS));
        assert!(chat.ends_with(",mcp__coppice__*"));

        assert_eq!(claude_allowed_tools(true, false), CHAT_READ_ONLY_TOOLS);
        assert_eq!(claude_allowed_tools(false, false), ALLOWED_TOOLS);
    }

    #[test]
    fn session_id_extracted_from_init_event() {
        let raw =
            std::fs::read_to_string(fixtures_root().join("done.jsonl")).expect("read done.jsonl");
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
        let mut console = crate::providers::claude_console::ClaudeConsolePublisher::new(
            crate::providers::ToolNameStyle::McpDoubleUnderscore,
        );
        for line in raw.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            console.handle_stream_json(handle, &value);
        }
    }

    fn collect_console_events(messages: &[crate::sessions::LiveMessage]) -> Vec<serde_json::Value> {
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

        let raw =
            std::fs::read_to_string(fixtures_root().join("done.jsonl")).expect("read done.jsonl");

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

        let raw =
            std::fs::read_to_string(fixtures_root().join("done.jsonl")).expect("read done.jsonl");

        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());

        publish_fixture_lines(&handle, &raw);

        let tail = handle.buffered_tail();
        assert_eq!(tail.len(), 4);
        assert!(tail.iter().all(|m| matches!(m, LiveMessage::Event { .. })));
    }
}
