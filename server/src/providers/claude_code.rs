use super::claude_console::ClaudeConsolePublisher;
use super::cli_runner::{
    log_launch, run_cli, run_cli_interactive, run_log_dir, CliError, CliInvocation, LineHandler,
    LineStep, RunIo,
};
use super::cli_version;
use super::terminal;
use super::{
    run_dir, worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult,
    ProviderError, CHAT_READ_ONLY_TOOLS,
};
use crate::mcp::grant::McpAccess;
use crate::mcp::wiring::{claude_tool_pattern, McpServerSpec};
use crate::services::ticket_attachments;
use crate::sessions::opencode_events::coppice_run_prompt;
use crate::sessions::run_registry::RunStreamHandle;
use async_trait::async_trait;
use coppice_config::ClaudeCodeProviderConfig;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const ALLOWED_TOOLS: &str =
    "Read,Write,Edit,MultiEdit,Bash,NotebookEdit,WebFetch,WebSearch,Glob,Grep,TodoWrite,Task";

/// Removed from the model on read-only runs. A bare deny drops the tool before
/// permission mode is considered. Chat uses `--permission-mode plan`, which
/// does not include Bash, Write, or Edit. `bypassPermissions` still approves
/// every tool an allowlist does not name, so ticket runs are the only path
/// that uses it.
const CHAT_DISALLOWED_TOOLS: &str = "Write,Edit,MultiEdit,NotebookEdit,Bash,TodoWrite,Task";

/// Inline settings for a chat process. Claude Code applies `--settings` above
/// user, project, and local files and below managed settings
/// (https://code.claude.com/docs/en/settings). `disableAllHooks: true` from
/// that layer turns off hooks in `~/.claude/settings.json`,
/// `.claude/settings.json`, and `.claude/settings.local.json`, plus plugin
/// hooks. Managed hooks still run; only managed settings can disable those
/// (https://code.claude.com/docs/en/settings-reference). `CLAUDE_CONFIG_DIR`
/// is left alone so login files under `~/.claude` stay in place.
const CHAT_SETTINGS_JSON: &str = r#"{"disableAllHooks":true}"#;

/// Drop project and local settings files (including their hooks). User
/// settings still load, and [`CHAT_SETTINGS_JSON`] disables the hooks in them.
const CHAT_SETTING_SOURCES: &str = "user";

fn push_chat_hook_lock(args: &mut Vec<String>) {
    args.push("--settings".to_string());
    args.push(CHAT_SETTINGS_JSON.to_string());
    args.push("--setting-sources".to_string());
    args.push(CHAT_SETTING_SOURCES.to_string());
}

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

fn push_tool_permission_args(args: &mut Vec<String>, read_only_tools: bool, mcp: bool) {
    args.push("--allowedTools".to_string());
    args.push(claude_allowed_tools(read_only_tools, mcp));
    if read_only_tools {
        args.push("--disallowedTools".to_string());
        args.push(CHAT_DISALLOWED_TOOLS.to_string());
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
        let version = cli_version::detect(&program).await.map_err(|err| {
            ProviderError::InvalidFixture(cli_version::version_unavailable(&program, &err))
        })?;
        let worktree_arg = worktree.display().to_string();
        let planning = crate::domain::workflow::is_read_only_job(&input.job_type);
        let read_only_tools = input.read_only_tools || planning;
        let staged = ticket_attachments::load_for_run(
            input.artifacts_dir.as_deref(),
            input.ticket_id.as_deref(),
        );
        let prompt = ticket_attachments::launch_prompt(coppice_run_prompt(), &staged);
        let argv = contract.argv(&coppice_connectors::LaunchSubst {
            read_only: read_only_tools,
            plan: planning,
            worktree: &worktree_arg,
            hostname: "",
            port: "",
            version: Some(version.as_str()),
        });
        // Read-only runs take the prompt on stdin, after a handshake that has
        // Claude Code ask Coppice before every tool (see `read_only_hook`).
        let mut args = if read_only_tools {
            let mut argv = argv;
            argv.extend(["--input-format".to_string(), "stream-json".to_string()]);
            argv
        } else {
            contract.with_prompt(argv, &prompt)
        };
        args.extend(ticket_attachments::add_dir_args(&staged));
        push_tool_permission_args(&mut args, read_only_tools, input.mcp.is_some());
        if input.context_profile == crate::domain::context_profile::ContextProfile::Conversation {
            push_chat_hook_lock(&mut args);
        }

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
            ticket_id: input.ticket_id.clone(),
            result_before_exit: contract.result_before_exit,
            result_exit_grace: None,
            stall_limits: None,
        };
        let mut handler = ClaudeLines {
            stream: input.stream.clone(),
            console: ClaudeConsolePublisher::new(super::tool_name_style(self.id())),
            assistant_text: String::new(),
            expects_gateway: input.mcp.is_some(),
            terminal: ClaudeTerminal::default(),
            tools: Default::default(),
            hook: read_only_tools.then(super::claude_hook::HookGate::default),
        };
        let io = RunIo {
            cancel_rx: input.cancel_rx,
            session_created_tx: input.session_created_tx,
        };

        let ran = if read_only_tools {
            run_cli_interactive(
                invocation,
                super::claude_hook::opening_lines(&prompt),
                &mut handler,
                io,
            )
            .await
        } else {
            run_cli(invocation, &mut handler, io).await
        };
        let exit = match ran {
            Ok(exit) => exit,
            Err(CliError::Spawn(e) | CliError::Io(e)) => return Err(ProviderError::Io(e)),
            Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
            Err(CliError::Stalled {
                reason,
                stderr_tail,
            }) => {
                return Err(super::terminal::stalled(
                    "Claude Code",
                    &reason,
                    &stderr_tail,
                ));
            }
            Err(CliError::TimedOut { stderr_tail }) => {
                return Err(terminal::timed_out(
                    NAME,
                    run_timeout.as_secs(),
                    &stderr_tail,
                ));
            }
        };
        decide(&handler, &exit, input.context_profile)
    }
}

/// The run's outcome once the CLI is done.
fn decide(
    handler: &ClaudeLines,
    exit: &super::cli_runner::CliExit,
    profile: crate::domain::context_profile::ContextProfile,
) -> Result<AgentRunResult, ProviderError> {
    let t = &handler.terminal;

    if let Some(status) = &t.gateway_failed {
        return Err(super::mcp_unavailable(&format!(
            "Claude Code couldn't connect to the Coppice gateway (MCP status: {status}){}",
            terminal::stderr_suffix(&exit.stderr_tail)
        )));
    }
    if !exit.status.success() && !exit.had_terminal_result {
        return Err(terminal::exited(NAME, &exit.status, &exit.stderr_tail));
    }
    // terminal_reason is checked on its own: prompt_too_long has been seen
    // arriving with is_error false.
    if t.terminal_reason.as_deref() == Some("prompt_too_long") {
        return Err(terminal::error_result(
            NAME,
            Some(&format!(
                "the session's context window is full (terminal_reason prompt_too_long){}",
                t.error_text
                    .as_deref()
                    .map(|text| format!(": {text}"))
                    .unwrap_or_default()
            )),
            &exit.stderr_tail,
        ));
    }
    if t.is_error {
        return Err(terminal::error_result(
            NAME,
            t.error_text.as_deref(),
            &exit.stderr_tail,
        ));
    }

    let stderr = terminal::stderr_suffix(&exit.stderr_tail);
    let mode = t.permission_mode.as_deref().unwrap_or("unknown");
    // A refused or failed submission is a broken setup, not a plain reply,
    // even when the agent explains it in text.
    let submit_problem = if let Some(error) = &t.submit_error {
        Some(format!(
            "Claude Code's result_submit call failed: {} (permission mode {mode}){stderr}",
            crate::util::truncate::truncate_with_ellipsis(error.trim(), 300)
        ))
    } else if t
        .denied_tools
        .iter()
        .any(|tool| tool.ends_with("result_submit"))
    {
        Some(format!(
            "Claude Code refused the result_submit tool (permission mode {mode}), so the result never reached Coppice{stderr}"
        ))
    } else {
        None
    };
    if let Some(problem) = submit_problem {
        return crate::sessions::opencode_events::extract_result_from_text(&handler.assistant_text)
            .ok_or(ProviderError::MissingResult(problem));
    }
    terminal::result_from_text(&handler.assistant_text, profile, NAME, || {
        format!("no result contract found in claude-code output{stderr}")
    })
}

const NAME: &str = "Claude Code";

/// What the stream said about how the turn went.
#[derive(Default)]
struct ClaudeTerminal {
    is_error: bool,
    terminal_reason: Option<String>,
    error_text: Option<String>,
    /// Tools the CLI refused (`permission_denials` on the result event).
    denied_tools: Vec<String>,
    /// The error a `result_submit` call came back with, when it failed.
    submit_error: Option<String>,
    submit_call_ids: Vec<String>,
    /// From the init event.
    permission_mode: Option<String>,
    /// The Coppice MCP server's status when it was not connected at startup.
    gateway_failed: Option<String>,
}

/// Forwards console events and accumulates assistant text; the `result`
/// event is terminal and its `result` field replaces the accumulated text.
struct ClaudeLines {
    stream: Option<Arc<RunStreamHandle>>,
    console: ClaudeConsolePublisher,
    assistant_text: String,
    expects_gateway: bool,
    terminal: ClaudeTerminal,
    tools: super::cli_runner::ToolTracker,
    /// Read-only runs: answers Claude Code's tool hooks.
    hook: Option<super::claude_hook::HookGate>,
}

/// Tool calls in flight: `tool_use` starts one, its `tool_result` ends it.
fn track_tools(tools: &mut super::cli_runner::ToolTracker, value: &serde_json::Value) {
    let Some(content) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return;
    };
    for part in content {
        match part.get("type").and_then(|t| t.as_str()) {
            Some("tool_use") => {
                if let Some(id) = part.get("id").and_then(|v| v.as_str()) {
                    tools.started(id);
                }
            }
            Some("tool_result") => {
                if let Some(id) = part.get("tool_use_id").and_then(|v| v.as_str()) {
                    tools.finished(id);
                }
            }
            _ => {}
        }
    }
}

impl LineHandler for ClaudeLines {
    fn tools_running(&self) -> bool {
        self.tools.any()
    }

    fn take_replies(&mut self) -> Vec<String> {
        self.hook
            .as_mut()
            .map(super::claude_hook::HookGate::take_replies)
            .unwrap_or_default()
    }

    fn on_json(&mut self, value: &serde_json::Value) -> LineStep {
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(stream) = &self.stream {
            if let Some(model) =
                crate::providers::reported_model::reported_model("claude-code", value)
            {
                stream.note_reported_model(&model);
            }
            self.console.handle_stream_json(stream, value);
        }

        if let Some(text) = extract_assistant_text(value) {
            self.assistant_text.push_str(&text);
        }

        let ty = value.get("type").and_then(|v| v.as_str());
        if let Some(hook) = self.hook.as_mut() {
            hook.on_json(value);
        }
        self.terminal.note_submit_attempts(value);
        track_tools(&mut self.tools, value);
        if ty == Some("system") && value.get("subtype").and_then(|v| v.as_str()) == Some("init") {
            self.terminal.permission_mode = value
                .get("permissionMode")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if self.expects_gateway {
                self.terminal.gateway_failed = gateway_failure(value);
                if self.terminal.gateway_failed.is_some() {
                    return LineStep {
                        session_id,
                        stop: true,
                    };
                }
            }
        }
        let stop = ty == Some("result") && !answers_task_notification(value);
        if stop {
            self.terminal.is_error = value.get("is_error").and_then(|v| v.as_bool()) == Some(true);
            self.terminal.terminal_reason = value
                .get("terminal_reason")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            self.terminal.denied_tools = value
                .get("permission_denials")
                .and_then(|v| v.as_array())
                .map(|denials| {
                    denials
                        .iter()
                        .filter_map(|d| d.get("tool_name").and_then(|v| v.as_str()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                if self.terminal.is_error {
                    self.terminal.error_text = Some(final_text.to_string());
                }
                self.assistant_text = final_text.to_string();
            }
        }
        LineStep { session_id, stop }
    }
}

/// A `result` that ends Claude Code's turn about a background task, not the
/// prompt's turn. Resuming a session whose background shell was still running
/// when the last run was stopped emits an empty one (origin
/// `task-notification`, `num_turns` 0) before the prompt even starts; ending
/// the run there reported "no result contract found". Seen on 2.1.310.
fn answers_task_notification(result: &serde_json::Value) -> bool {
    result.pointer("/origin/kind").and_then(|kind| kind.as_str()) == Some("task-notification")
}

impl ClaudeTerminal {
    /// Remembers `result_submit` tool calls and the error one came back with.
    fn note_submit_attempts(&mut self, value: &serde_json::Value) {
        let Some(content) = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return;
        };
        for part in content {
            match part.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => {
                    let is_submit = part
                        .get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|name| name.ends_with("result_submit"));
                    if let (true, Some(id)) = (is_submit, part.get("id").and_then(|v| v.as_str())) {
                        self.submit_call_ids.push(id.to_string());
                    }
                }
                Some("tool_result") => {
                    let ours = part
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|id| self.submit_call_ids.iter().any(|c| c == id));
                    let failed = part.get("is_error").and_then(|v| v.as_bool()) == Some(true);
                    if ours && failed {
                        self.submit_error = Some(tool_result_text(part));
                    } else if ours {
                        self.submit_error = None;
                    }
                }
                _ => {}
            }
        }
    }
}

/// A tool result's text, whether `content` is a string or a list of parts.
fn tool_result_text(part: &serde_json::Value) -> String {
    match part.get("content") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The Coppice server's status from the init event's `mcp_servers`, when it
/// failed. `pending` is still connecting and is not a failure; a missing entry
/// means the config never loaded it.
fn gateway_failure(init: &serde_json::Value) -> Option<String> {
    let servers = init.get("mcp_servers")?.as_array()?;
    let ours = servers.iter().find(|server| {
        server.get("name").and_then(|v| v.as_str()) == Some(crate::mcp::protocol::SERVER_NAME)
    });
    match ours
        .and_then(|server| server.get("status"))
        .and_then(|v| v.as_str())
    {
        None if ours.is_none() => Some("not loaded".into()),
        Some("connected") | Some("pending") | None => None,
        Some(other) => Some(other.to_string()),
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
    use crate::sessions::opencode_events::extract_result_from_text;
    use std::path::PathBuf;

    mod outcome {
        use super::super::*;
        use crate::domain::context_profile::ContextProfile;
        use serde_json::json;
        use std::os::unix::process::ExitStatusExt;

        fn handler(gateway: bool) -> ClaudeLines {
            ClaudeLines {
                stream: None,
                console: ClaudeConsolePublisher::new(super::super::super::tool_name_style(
                    "claude-code",
                )),
                assistant_text: String::new(),
                expects_gateway: gateway,
                terminal: ClaudeTerminal::default(),
                tools: Default::default(),
                hook: None,
            }
        }

        fn exit(code: i32, stderr: &[&str]) -> super::super::super::cli_runner::CliExit {
            super::super::super::cli_runner::CliExit {
                status: std::process::ExitStatus::from_raw(code << 8),
                stderr_tail: stderr.iter().map(|s| s.to_string()).collect(),
                had_terminal_result: false,
                stopped_after_result: false,
            }
        }

        fn init(mode: &str, status: &str) -> serde_json::Value {
            json!({ "type": "system", "subtype": "init", "permissionMode": mode,
                    "mcp_servers": [{ "name": "coppice", "status": status }] })
        }

        fn result(text: &str, is_error: bool, reason: &str) -> serde_json::Value {
            json!({ "type": "result", "subtype": "success", "is_error": is_error,
                    "terminal_reason": reason, "result": text, "permission_denials": [] })
        }

        fn feed(h: &mut ClaudeLines, lines: &[serde_json::Value]) {
            for line in lines {
                h.on_json(line);
            }
        }

        #[test]
        fn a_refused_submit_names_the_tool_error_even_in_chat() {
            // The rc.8 shape: plan mode refused the MCP call; the agent explained.
            let mut h = handler(true);
            feed(
                &mut h,
                &[
                    init("plan", "connected"),
                    json!({ "type": "assistant", "message": { "content": [
                    { "type": "tool_use", "id": "toolu_1", "name": "mcp__coppice__result_submit", "input": {} } ] } }),
                    json!({ "type": "user", "message": { "content": [
                    { "type": "tool_result", "tool_use_id": "toolu_1", "is_error": true,
                      "content": "Cannot call mcp__coppice__result_submit while in plan mode." } ] } }),
                    result(
                        "I couldn't submit the reply because plan mode is on.",
                        false,
                        "completed",
                    ),
                ],
            );
            let err = decide(&h, &exit(0, &[]), ContextProfile::Conversation).unwrap_err();
            let msg = err.to_string();
            assert!(matches!(err, ProviderError::MissingResult(_)), "{msg}");
            assert!(
                msg.contains("result_submit call failed: Cannot call"),
                "{msg}"
            );
            assert!(msg.contains("permission mode plan"), "{msg}");
        }

        #[test]
        fn a_later_successful_submit_clears_the_error() {
            let mut h = handler(true);
            feed(
                &mut h,
                &[
                    init("dontAsk", "connected"),
                    json!({ "type": "assistant", "message": { "content": [
                    { "type": "tool_use", "id": "a", "name": "mcp__coppice__result_submit" },
                    { "type": "tool_use", "id": "b", "name": "mcp__coppice__result_submit" } ] } }),
                    json!({ "type": "user", "message": { "content": [
                    { "type": "tool_result", "tool_use_id": "a", "is_error": true, "content": [{ "type": "text", "text": "bad args" }] },
                    { "type": "tool_result", "tool_use_id": "b", "content": "ok" } ] } }),
                    result("Done.", false, "completed"),
                ],
            );
            let reply =
                decide(&h, &exit(0, &[]), ContextProfile::Conversation).expect("plain reply");
            assert!(matches!(reply, AgentRunResult::Done { .. }));
        }

        #[test]
        fn a_permission_denial_of_submit_is_named() {
            let mut h = handler(true);
            let mut done = result("text", false, "completed");
            done["permission_denials"] =
                json!([{ "tool_name": "mcp__coppice__result_submit", "tool_use_id": "x" }]);
            feed(&mut h, &[init("default", "connected"), done]);
            let msg = decide(&h, &exit(0, &[]), ContextProfile::Full)
                .unwrap_err()
                .to_string();
            assert!(
                msg.contains("refused the result_submit tool (permission mode default)"),
                "{msg}"
            );
        }

        #[test]
        fn a_gateway_that_did_not_connect_stops_the_run_early() {
            let mut h = handler(true);
            let step = h.on_json(&init("dontAsk", "failed"));
            assert!(step.stop);
            let msg = decide(
                &h,
                &exit(0, &["mcp: connection refused"]),
                ContextProfile::Full,
            )
            .unwrap_err()
            .to_string();
            assert!(msg.contains("mcp_unavailable: Claude Code couldn't connect to the Coppice gateway (MCP status: failed)"), "{msg}");
            assert!(msg.contains("connection refused"), "{msg}");

            let mut h = handler(true);
            assert!(!h.on_json(&init("dontAsk", "pending")).stop);
            let mut missing = init("dontAsk", "connected");
            missing["mcp_servers"] = json!([]);
            assert!(h.on_json(&missing).stop, "a gateway that never loaded");
            // No gateway was asked for: its absence is fine.
            let mut h = handler(false);
            assert!(!h.on_json(&missing).stop);
        }

        #[test]
        fn an_error_result_uses_the_cli_text() {
            let mut h = handler(false);
            feed(
                &mut h,
                &[result("API Error: 529 Overloaded", true, "api_error")],
            );
            let err = decide(&h, &exit(0, &[]), ContextProfile::Full).unwrap_err();
            assert!(matches!(err, ProviderError::MissingResult(_)));
            assert!(
                err.to_string()
                    .contains("Claude Code ended with an error: API Error: 529 Overloaded"),
                "{err}"
            );
        }

        #[test]
        fn a_full_context_fails_even_without_the_error_flag() {
            let mut h = handler(false);
            feed(&mut h, &[result("", false, "prompt_too_long")]);
            let msg = decide(&h, &exit(0, &[]), ContextProfile::Full)
                .unwrap_err()
                .to_string();
            assert!(msg.contains("context window is full"), "{msg}");
        }

        #[test]
        fn plain_text_is_a_chat_reply_but_not_a_ticket_result() {
            let mut h = handler(false);
            feed(&mut h, &[result("Hi there!", false, "completed")]);
            assert!(decide(&h, &exit(0, &[]), ContextProfile::Conversation).is_ok());
            let msg = decide(&h, &exit(0, &[]), ContextProfile::Full)
                .unwrap_err()
                .to_string();
            assert!(
                msg.contains("no result contract found in claude-code output"),
                "{msg}"
            );
        }

        #[test]
        fn a_tool_counts_as_running_until_its_result() {
            let mut h = handler(false);
            assert!(!h.tools_running());
            h.on_json(&json!({ "type": "assistant", "message": { "content": [
                { "type": "tool_use", "id": "b1", "name": "Bash", "input": { "command": "cargo test" } } ] } }));
            assert!(h.tools_running());
            h.on_json(&json!({ "type": "user", "message": { "content": [
                { "type": "tool_result", "tool_use_id": "b1", "content": "ok" } ] } }));
            assert!(!h.tools_running());
        }

        fn replay(name: &str) -> ClaudeLines {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../fixtures/claude-code")
                .join(name);
            let mut h = handler(true);
            for line in std::fs::read_to_string(path).expect("fixture").lines() {
                h.on_json(&serde_json::from_str(line).expect("json line"));
            }
            h
        }

        #[test]
        fn a_task_notification_result_on_resume_does_not_end_the_run() {
            let mut h = handler(false);
            let contract = r#"```json
{"status":"done","summary":"Did the work."}
```"#;
            // Claude Code 2.1.310 resuming a session whose background shell
            // was still running when the previous run was stopped.
            let notification = json!({ "type": "system", "subtype": "task_notification",
                "status": "stopped",
                "summary": "Background shell command didn't finish before the previous session ended" });
            let notification_result = json!({ "type": "result", "subtype": "success",
                "is_error": false, "num_turns": 0, "stop_reason": null, "result": "",
                "origin": { "kind": "task-notification" }, "permission_denials": [] });
            for line in [notification, init("bypassPermissions", "connected"), notification_result] {
                assert!(!h.on_json(&line).stop, "{line}");
            }
            let assistant = json!({ "type": "assistant", "message": { "content": [
                { "type": "text", "text": contract } ] } });
            assert!(!h.on_json(&init("bypassPermissions", "connected")).stop);
            assert!(!h.on_json(&assistant).stop);
            assert!(h.on_json(&result(contract, false, "completed")).stop);

            match decide(&h, &exit(0, &[]), ContextProfile::Full).expect("result") {
                AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Did the work."),
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn recorded_2_1_302_dont_ask_run_submits_and_ends_cleanly() {
            let h = replay("claude-code-2.1.302-dontask-submit.jsonl");
            assert!(h.terminal.gateway_failed.is_none());
            assert_eq!(h.terminal.permission_mode.as_deref(), Some("dontAsk"));
            assert!(h.terminal.submit_error.is_none());
            assert!(!h.tools_running());
            // Its final words are prose; a chat keeps them, a ticket needs the submission.
            assert!(decide(&h, &exit(0, &[]), ContextProfile::Conversation).is_ok());
            assert!(matches!(
                decide(&h, &exit(0, &[]), ContextProfile::Full),
                Err(ProviderError::MissingResult(_))
            ));
        }

        #[test]
        fn recorded_2_1_302_plan_mode_refusal_is_named() {
            let h = replay("claude-code-2.1.302-plan-submit-refused.jsonl");
            assert_eq!(h.terminal.permission_mode.as_deref(), Some("plan"));
            assert!(h
                .terminal
                .denied_tools
                .iter()
                .any(|t| t == "mcp__coppice__result_submit"));
            let msg = decide(&h, &exit(0, &[]), ContextProfile::Conversation)
                .unwrap_err()
                .to_string();
            assert!(
                msg.contains("Cannot call mcp__coppice__result_submit while in plan mode"),
                "{msg}"
            );
            assert!(msg.contains("permission mode plan"), "{msg}");
        }

        #[test]
        fn a_read_only_run_answers_tool_hooks_through_the_handler() {
            let mut h = handler(true);
            h.hook = Some(Default::default());
            h.on_json(&json!({ "type": "control_request", "request_id": "q1",
                "request": { "subtype": "hook_callback", "callback_id": "coppice-pre-tool-use",
                             "input": { "tool_name": "Write" } } }));
            let replies = h.take_replies();
            assert_eq!(replies.len(), 1);
            assert!(replies[0].contains("\"permissionDecision\":\"deny\""), "{}", replies[0]);
            assert!(h.take_replies().is_empty());
            // A write run has no gate and writes nothing back.
            let mut write = handler(false);
            write.on_json(&json!({ "type": "control_request", "request_id": "q2", "request": {} }));
            assert!(write.take_replies().is_empty());
        }

        #[test]
        fn a_crash_before_any_result_carries_stderr() {
            let h = handler(false);
            let err = decide(
                &h,
                &exit(1, &["Error: not logged in"]),
                ContextProfile::Full,
            )
            .unwrap_err();
            assert!(matches!(err, ProviderError::InvalidFixture(_)));
            assert!(err.to_string().contains("not logged in"), "{err}");
        }
    }

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

    fn chat_argv(read_only: bool, plan: bool, mcp: bool) -> Vec<String> {
        let contract = coppice_connectors::get(coppice_connectors::CLAUDE_CODE)
            .expect("claude-code descriptor")
            .run_contract;
        let mut args = contract.with_prompt(
            // Same as `run()`: planning is always read-only.
            contract.argv(&coppice_connectors::LaunchSubst {
                read_only: read_only || plan,
                plan,
                worktree: "/tmp/chat",
                hostname: "",
                port: "",
                version: Some("2.1.292"),
            }),
            "prompt",
        );
        push_tool_permission_args(&mut args, read_only || plan, mcp);
        // Chat is read-only and not a plan run. Plan and ticket runs leave hooks alone.
        if read_only && !plan {
            push_chat_hook_lock(&mut args);
        }
        args
    }

    fn hook_commands(settings: &serde_json::Value) -> Vec<String> {
        let mut commands = Vec::new();
        let Some(events) = settings.get("hooks").and_then(|v| v.as_object()) else {
            return commands;
        };
        for groups in events.values().filter_map(|v| v.as_array()) {
            for group in groups {
                let Some(hooks) = group.get("hooks").and_then(|v| v.as_array()) else {
                    continue;
                };
                for hook in hooks {
                    if let Some(command) = hook.get("command").and_then(|v| v.as_str()) {
                        commands.push(command.to_string());
                    }
                }
            }
        }
        commands
    }

    /// `--settings` wins over the same key in user settings. A true value
    /// disables the hook commands that file still contains.
    fn enabled_hook_commands(
        user_settings: &serde_json::Value,
        cli_settings: &serde_json::Value,
    ) -> Vec<String> {
        let disabled = if let Some(value) = cli_settings.get("disableAllHooks") {
            value.as_bool() == Some(true)
        } else {
            user_settings
                .get("disableAllHooks")
                .and_then(|v| v.as_bool())
                == Some(true)
        };
        if disabled {
            Vec::new()
        } else {
            hook_commands(user_settings)
        }
    }

    fn flag_value<'a>(args: &'a [String], flag: &str) -> &'a str {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .unwrap_or_else(|| panic!("missing {flag} in {args:?}"))[1]
            .as_str()
    }

    #[test]
    fn read_only_chat_blocks_write_edit_and_bash() {
        let args = chat_argv(true, false, false);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--permission-mode", "dontAsk"]),
            "{args:?}"
        );
        assert!(
            !args.iter().any(|arg| arg == "bypassPermissions"),
            "bypassPermissions approves tools outside --allowedTools: {args:?}"
        );
        assert_eq!(flag_value(&args, "--allowedTools"), CHAT_READ_ONLY_TOOLS);
        assert_eq!(flag_value(&args, "--settings"), CHAT_SETTINGS_JSON);
        assert_eq!(flag_value(&args, "--setting-sources"), CHAT_SETTING_SOURCES);
        let disallowed: Vec<_> = flag_value(&args, "--disallowedTools").split(',').collect();
        for tool in [
            "Write",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "Bash",
            "TodoWrite",
            "Task",
        ] {
            assert!(
                disallowed.contains(&tool),
                "{tool} must be disallowed, got {disallowed:?}"
            );
            assert!(
                !CHAT_READ_ONLY_TOOLS.split(',').any(|name| name == tool),
                "{tool} must not be allowed"
            );
        }
    }

    #[test]
    fn write_runs_keep_bypass_and_do_not_disallow_edit_tools() {
        let args = chat_argv(false, false, false);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "bypassPermissions"]));
        assert!(!args.iter().any(|arg| arg == "--disallowedTools"));
        assert!(!args.iter().any(|arg| arg == "--settings"));
        assert!(!args.iter().any(|arg| arg == "--setting-sources"));
        assert_eq!(flag_value(&args, "--allowedTools"), ALLOWED_TOOLS);
    }

    #[test]
    fn chat_settings_disable_a_user_level_hook() {
        let raw = std::fs::read_to_string(fixtures_root().join("user-settings-with-hook.json"))
            .expect("user hook fixture");
        let user: serde_json::Value = serde_json::from_str(&raw).expect("parse user settings");
        let stored = hook_commands(&user);
        assert_eq!(
            stored,
            vec!["touch \"$HOME/coppice-user-hook-ran\"".to_string()]
        );
        assert_eq!(
            enabled_hook_commands(&user, &serde_json::json!({})),
            stored,
            "the fixture hook runs when chat does not pass --settings"
        );

        let args = chat_argv(true, false, false);
        let cli: serde_json::Value =
            serde_json::from_str(flag_value(&args, "--settings")).expect("chat --settings json");
        assert_eq!(cli["disableAllHooks"], true);
        assert!(
            enabled_hook_commands(&user, &cli).is_empty(),
            "user hook must be disabled: {stored:?}"
        );
    }

    #[test]
    fn chat_argv_strict_mcp_config_is_coppice_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let run_dir = tmp.path().join("runs").join("chat-1");
        // Same append `run()` makes when a chat turn has gateway access.
        let mut args = chat_argv(true, false, true);
        args.extend(claude_mcp_args(&access(), &run_dir).expect("mcp args"));

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "dontAsk"]));
        assert_eq!(
            args.iter()
                .filter(|arg| arg.as_str() == "--strict-mcp-config")
                .count(),
            1,
            "{args:?}"
        );
        let raw = std::fs::read_to_string(flag_value(&args, "--mcp-config")).expect("mcp.json");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("parse mcp.json");
        let servers = doc["mcpServers"].as_object().expect("mcpServers object");
        assert_eq!(servers.keys().cloned().collect::<Vec<_>>(), vec!["coppice"]);
        assert_eq!(servers["coppice"]["url"], "http://127.0.0.1:5000/mcp");
        assert!(!raw.contains("super-secret-run-token"));
        assert!(flag_value(&args, "--allowedTools").ends_with(",mcp__coppice__*"));
        assert!(!args.iter().any(|arg| arg == "open_repo"));
    }

    #[test]
    fn plan_runs_disallow_write_tools_without_bypass() {
        let args = chat_argv(false, true, true);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "dontAsk"]));
        assert!(!args.iter().any(|arg| arg == "bypassPermissions"));
        assert!(flag_value(&args, "--disallowedTools")
            .split(',')
            .any(|tool| tool == "Bash"));
        assert!(
            !args.iter().any(|arg| arg == "--settings"),
            "plan runs are not chat: {args:?}"
        );
        assert!(flag_value(&args, "--allowedTools").ends_with(",mcp__coppice__*"));
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
