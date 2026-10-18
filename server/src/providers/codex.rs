use super::cli_runner::{
    log_launch, run_cli_with_stdin, run_log_dir, CliError, CliInvocation, LineHandler, LineStep,
    RunIo,
};
use super::cli_version;
use super::codex_console::CodexConsolePublisher;
use super::terminal;
use super::{
    worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult, ProviderError,
};
use crate::mcp::grant::McpAccess;
use crate::mcp::wiring::McpServerSpec;
use crate::services::ticket_attachments;
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
fn append_model_flag(args: &mut Vec<String>, model: Option<&str>) {
    crate::providers::model_flag::push_model_flag(args, "-m", model);
}

fn codex_mcp_args(access: &McpAccess) -> Vec<String> {
    McpServerSpec::from_access(access).codex_args()
}

/// Chat skips `$CODEX_HOME/config.toml`. Auth still uses `CODEX_HOME`
/// (`codex exec --help` on 0.162.1). Project `.codex/config.toml` loads only
/// when that user config trusts the project, so it is skipped as well.
/// `-c mcp_servers={}` does not clear the table on 0.162.1; Coppice is added
/// afterwards with its own `-c` keys, which is the allowlist.
fn push_chat_mcp_isolation(args: &mut Vec<String>) {
    args.push("--ignore-user-config".to_string());
}

struct CodexLaunch<'a> {
    version: &'a str,
    worktree: &'a str,
    read_only: bool,
    plan: bool,
    model: Option<&'a str>,
    chat: bool,
    mcp: Option<&'a McpAccess>,
    resume: Option<&'a str>,
}

fn codex_launch_args(
    contract: coppice_connectors::RunContract,
    launch: CodexLaunch<'_>,
) -> Vec<String> {
    let mut args = contract.argv(&coppice_connectors::LaunchSubst {
        read_only: launch.read_only,
        plan: launch.plan,
        worktree: launch.worktree,
        hostname: "",
        port: "",
        version: Some(launch.version),
    });
    append_model_flag(&mut args, launch.model);
    if launch.chat {
        push_chat_mcp_isolation(&mut args);
    }
    if let Some(access) = launch.mcp {
        args.extend(codex_mcp_args(access));
    }
    if let Some(sid) = launch.resume.filter(|sid| !sid.is_empty()) {
        args.push("resume".to_string());
        args.push(sid.to_string());
    }
    args
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
        let descriptor =
            coppice_connectors::get(coppice_connectors::CODEX).expect("codex descriptor");
        let contract = descriptor.run_contract;
        let program = self.config.command.clone();
        let version = cli_version::detect(&program).await.map_err(|err| {
            ProviderError::InvalidFixture(cli_version::version_unavailable(&program, &err))
        })?;
        let worktree_arg = worktree.display().to_string();
        // The prompt stays on stdin. Pinned flags, including a version-gated
        // `--no-daemon`, come from the descriptor. Isolation and the Coppice
        // MCP allowlist are applied before `resume` so they bind to `exec`.
        let chat =
            input.context_profile == crate::domain::context_profile::ContextProfile::Conversation;
        let staged = ticket_attachments::load_for_run(
            input.artifacts_dir.as_deref(),
            input.ticket_id.as_deref(),
        );
        let mut args = codex_launch_args(
            contract,
            CodexLaunch {
                version: &version,
                worktree: &worktree_arg,
                read_only: input.read_only_tools,
                plan: crate::domain::workflow::is_read_only_job(&input.job_type),
                model: input.model.as_deref(),
                chat,
                mcp: input.mcp.as_ref(),
                resume: input.resume_session_id.as_deref(),
            },
        );
        args.extend(ticket_attachments::add_dir_args(&staged));
        args.extend(ticket_attachments::image_input_args(&staged));
        let mut env = Vec::new();
        if let Some(access) = &input.mcp {
            env.extend(access.env().map(|(k, v)| (k.to_string(), v)));
        }

        // Auth is host-managed: the operator runs `codex login` wherever the server runs.
        // The child process inherits that environment directly — same model as claude-code
        // and opencode. Coppice does not inject or strip credentials.

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
                // Codex takes its root from `-C`; the process keeps the server's cwd.
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
        let mut handler = CodexLines {
            stream: input.stream.clone(),
            console: CodexConsolePublisher::new(super::tool_name_style(self.id())),
            assistant_text: String::new(),
            last_message: String::new(),
            turn_failed: None,
            last_error: None,
            tools: Default::default(),
        };
        let io = RunIo {
            cancel_rx: input.cancel_rx,
            session_created_tx: input.session_created_tx,
        };

        let exit = match run_cli_with_stdin(
            invocation,
            Some(ticket_attachments::launch_prompt(
                coppice_run_prompt(),
                &staged,
            )),
            &mut handler,
            io,
        )
        .await
        {
            Ok(exit) => exit,
            Err(CliError::Spawn(e) | CliError::Io(e)) => return Err(ProviderError::Io(e)),
            Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
            Err(CliError::Stalled { reason, stderr_tail }) => {
                return Err(super::terminal::stalled("Codex", &reason, &stderr_tail));
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

const NAME: &str = "Codex";

/// The run's outcome once the CLI is done.
fn decide(
    handler: &CodexLines,
    exit: &super::cli_runner::CliExit,
    profile: crate::domain::context_profile::ContextProfile,
) -> Result<AgentRunResult, ProviderError> {
    if let Some(reason) = &handler.turn_failed {
        let detail = reason.as_deref().or(handler.last_error.as_deref());
        return Err(terminal::error_result(NAME, detail, &exit.stderr_tail));
    }
    if !exit.status.success() && !exit.had_terminal_result {
        return Err(terminal::exited(NAME, &exit.status, &exit.stderr_tail));
    }
    // A contract may span several messages; a plain chat reply is the last one.
    if let Some(result) = extract_result_from_text(&handler.assistant_text) {
        return Ok(result);
    }
    let stderr = terminal::stderr_suffix(&exit.stderr_tail);
    terminal::result_from_text(&handler.last_message, profile, NAME, || {
        format!("no result contract found in codex output{stderr}")
    })
}

/// Forwards console events and accumulates agent messages; `turn.completed`
/// is terminal. Codex reports its session as `thread_id`.
struct CodexLines {
    stream: Option<Arc<RunStreamHandle>>,
    console: CodexConsolePublisher,
    assistant_text: String,
    last_message: String,
    /// `turn.failed`, with its message when it had one.
    turn_failed: Option<Option<String>>,
    /// The latest `error` event. Codex also reports retries this way, so it
    /// only explains a failure; it does not cause one.
    last_error: Option<String>,
    tools: super::cli_runner::ToolTracker,
}

impl LineHandler for CodexLines {
    fn tools_running(&self) -> bool {
        self.tools.any()
    }

    fn on_json(&mut self, value: &serde_json::Value) -> LineStep {
        let session_id = value
            .get("thread_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(stream) = &self.stream {
            if let Some(model) = crate::providers::reported_model::reported_model("codex", value) {
                stream.note_reported_model(&model);
            }
            self.console.handle_json(stream, value);
        }

        if let Some(text) = extract_assistant_text(value) {
            self.assistant_text.push_str(&text);
            self.last_message = text;
        }
        let ty = value.get("type").and_then(|v| v.as_str());
        if let (Some(kind), Some(id)) = (
            value.pointer("/item/type").and_then(|v| v.as_str()),
            value.pointer("/item/id").and_then(|v| v.as_str()),
        ) {
            let is_tool = matches!(
                kind,
                "command_execution" | "mcp_tool_call" | "web_search" | "file_change"
            );
            match ty {
                Some("item.started") if is_tool => self.tools.started(id),
                Some("item.completed") => self.tools.finished(id),
                _ => {}
            }
        }
        match ty {
            Some("turn.failed") => {
                self.turn_failed = Some(
                    value
                        .pointer("/error/message")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                );
            }
            Some("error") => {
                self.last_error = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            _ => {}
        }

        LineStep {
            session_id,
            stop: matches!(ty, Some("turn.completed") | Some("turn.failed")),
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

    mod outcome {
        use super::super::*;
        use crate::domain::context_profile::ContextProfile;
        use serde_json::json;
        use std::os::unix::process::ExitStatusExt;

        fn run(lines: &[serde_json::Value], code: i32, stderr: &[&str], profile: ContextProfile) -> Result<AgentRunResult, ProviderError> {
            let mut handler = CodexLines {
                stream: None,
                console: CodexConsolePublisher::new(super::super::super::tool_name_style("codex")),
                assistant_text: String::new(),
                last_message: String::new(),
                turn_failed: None,
                last_error: None,
                tools: Default::default(),
            };
            let mut stopped = false;
            for line in lines {
                stopped = handler.on_json(line).stop;
            }
            let exit = super::super::super::cli_runner::CliExit {
                status: std::process::ExitStatus::from_raw(code << 8),
                stderr_tail: stderr.iter().map(|s| s.to_string()).collect(),
                had_terminal_result: stopped,
                stopped_after_result: false,
            };
            decide(&handler, &exit, profile)
        }

        fn message(id: &str, text: &str) -> serde_json::Value {
            json!({ "type": "item.completed", "item": { "id": id, "type": "agent_message", "text": text } })
        }

        #[test]
        fn a_failed_turn_fails_with_its_message() {
            let err = run(
                &[json!({ "type": "turn.failed", "error": { "message": "usage limit reached" } })],
                1,
                &["warn: quota"],
                ContextProfile::Full,
            )
            .unwrap_err();
            assert!(matches!(err, ProviderError::MissingResult(_)));
            assert!(err.to_string().contains("Codex ended with an error: usage limit reached (stderr: warn: quota)"), "{err}");
        }

        #[test]
        fn a_failed_turn_without_a_message_falls_back_to_the_last_error_event() {
            let err = run(
                &[json!({ "type": "error", "message": "stream disconnected" }), json!({ "type": "turn.failed" })],
                0,
                &[],
                ContextProfile::Full,
            )
            .unwrap_err();
            assert!(err.to_string().contains("stream disconnected"), "{err}");
        }

        #[test]
        fn a_retry_error_event_alone_is_not_a_failure() {
            let done = r#"{"status":"done","summary":"ok"}"#;
            let result = run(
                &[json!({ "type": "error", "message": "Reconnecting... 1/5" }), message("m1", done), json!({ "type": "turn.completed" })],
                0,
                &[],
                ContextProfile::Full,
            );
            assert!(result.is_ok(), "{result:?}");
        }

        #[test]
        fn chat_replies_with_the_last_message_not_the_progress_notes() {
            let reply = run(
                &[message("m1", "Let me look around."), message("m2", "Here is the answer."), json!({ "type": "turn.completed" })],
                0,
                &[],
                ContextProfile::Conversation,
            )
            .expect("reply");
            match reply {
                AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Here is the answer."),
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn a_contract_split_over_messages_still_parses() {
            let result = run(
                &[message("m1", r#"{"status":"done","#), message("m2", r#""summary":"joined"}"#), json!({ "type": "turn.completed" })],
                0,
                &[],
                ContextProfile::Full,
            )
            .expect("contract");
            assert!(matches!(result, AgentRunResult::Done { .. }));
        }

        #[test]
        fn commands_count_as_running_until_completed() {
            let mut handler = CodexLines {
                stream: None,
                console: CodexConsolePublisher::new(super::super::super::tool_name_style("codex")),
                assistant_text: String::new(),
                last_message: String::new(),
                turn_failed: None,
                last_error: None,
                tools: Default::default(),
            };
            handler.on_json(&json!({ "type": "item.started", "item": { "id": "m", "type": "agent_message" } }));
            assert!(!handler.tools_running());
            handler.on_json(&json!({ "type": "item.started", "item": { "id": "c1", "type": "command_execution", "command": "make" } }));
            assert!(handler.tools_running());
            handler.on_json(&json!({ "type": "item.completed", "item": { "id": "c1", "type": "command_execution" } }));
            assert!(!handler.tools_running());
        }

        #[test]
        fn an_exit_before_finishing_carries_stderr() {
            let err = run(&[], 2, &["error: unexpected argument '--foo'"], ContextProfile::Full).unwrap_err();
            assert!(matches!(err, ProviderError::InvalidFixture(_)));
            assert!(err.to_string().contains("unexpected argument"), "{err}");
        }
    }

    #[test]
    fn blank_model_omits_model_flag_from_launched_command() {
        let contract = coppice_connectors::get(coppice_connectors::CODEX)
            .expect("codex descriptor")
            .run_contract;
        let base = contract.argv(&coppice_connectors::LaunchSubst {
            read_only: false,
            plan: false,
            worktree: "/tmp/wt",
            hostname: "",
            port: "",
            version: Some("0.156.0"),
        });
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

    fn access() -> crate::mcp::grant::McpAccess {
        crate::mcp::grant::McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "super-secret-run-token".into(),
        }
    }

    fn launch(chat: bool, read_only: bool, mcp: bool) -> Vec<String> {
        let contract = coppice_connectors::get(coppice_connectors::CODEX)
            .expect("codex descriptor")
            .run_contract;
        let access = access();
        codex_launch_args(
            contract,
            CodexLaunch {
                version: "0.162.1",
                worktree: "/tmp/chat",
                read_only,
                plan: false,
                model: None,
                chat,
                mcp: if mcp { Some(&access) } else { None },
                resume: None,
            },
        )
    }

    fn server_names(doc: &toml::Value) -> std::collections::BTreeSet<String> {
        doc.get("mcp_servers")
            .and_then(|value| value.as_table())
            .map(|table| table.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 0.162.1 merges `-c` into the loaded tables. `--ignore-user-config` loads
    /// neither the user file nor a project file whose trust entry lived there.
    /// An empty `mcp_servers={}` override does not remove servers.
    fn effective_mcp_servers(user_toml: &str, project_toml: &str, args: &[String]) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        if !args.iter().any(|arg| arg == "--ignore-user-config") {
            let user: toml::Value = toml::from_str(user_toml).expect("user config");
            let project: toml::Value = toml::from_str(project_toml).expect("project config");
            names.extend(server_names(&user));
            names.extend(server_names(&project));
        }
        for pair in args.windows(2) {
            if pair[0] != "-c" {
                continue;
            }
            let Some(rest) = pair[1].strip_prefix("mcp_servers.") else {
                continue;
            };
            let Some(name) = rest.split(['.', '=']).next() else {
                continue;
            };
            if !name.is_empty() && !name.starts_with('{') {
                names.insert(name.to_string());
            }
        }
        names.into_iter().collect()
    }

    #[test]
    fn chat_allowlists_coppice_and_drops_other_mcp_servers() {
        let user = std::fs::read_to_string(fixtures_root().join("user-config-with-mcp.toml"))
            .expect("user mcp fixture");
        let project = std::fs::read_to_string(fixtures_root().join("project-config-with-mcp.toml"))
            .expect("project mcp fixture");
        let user_doc: toml::Value = toml::from_str(&user).expect("user toml");
        let project_doc: toml::Value = toml::from_str(&project).expect("project toml");
        assert!(server_names(&user_doc).contains("user_evil"));
        assert!(server_names(&project_doc).contains("project_evil"));

        let ticket = launch(false, false, true);
        assert!(ticket
            .iter()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"));
        assert!(!ticket.iter().any(|arg| arg == "--ignore-user-config"));
        assert_eq!(
            effective_mcp_servers(&user, &project, &ticket),
            vec![
                "coppice".to_string(),
                "project_evil".to_string(),
                "user_evil".to_string()
            ]
        );

        let args = launch(true, true, true);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"]));
        assert!(!args
            .iter()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"));
        assert!(args.iter().any(|arg| arg == "--ignore-user-config"));
        assert!(args
            .iter()
            .any(|arg| arg.contains("mcp_servers.coppice.url=")));
        assert!(args
            .iter()
            .any(|arg| { arg.contains("mcp_servers.coppice.bearer_token_env_var=") }));
        assert!(!args
            .iter()
            .any(|arg| arg.contains("user_evil") || arg.contains("project_evil")));
        assert_eq!(
            effective_mcp_servers(&user, &project, &args),
            vec!["coppice".to_string()]
        );
        assert!(!args.iter().any(|arg| arg.contains("open_repo")));
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
