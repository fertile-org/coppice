use super::cli_runner::{
    log_launch, run_cli, run_log_dir, CliError, CliExit, CliInvocation, LineHandler, LineStep,
    RunIo,
};
use super::cli_version;
use super::cursor_console::CursorConsolePublisher;
use super::{
    artifacts_path, mcp_unavailable, worktree_dir_from_context, AgentProvider, AgentRunInput,
    AgentRunResult, ProviderError,
};
use crate::mcp::grant::McpAccess;
use crate::mcp::wiring::McpServerSpec;
use crate::services::ticket_attachments;
use crate::sessions::opencode_events::coppice_run_prompt;
use crate::sessions::run_registry::RunStreamHandle;
use async_trait::async_trait;
use coppice_config::CursorProviderConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        let command = self.config.command.as_str();
        let contract = coppice_connectors::get(coppice_connectors::CURSOR)
            .expect("cursor descriptor")
            .run_contract;
        let version = cli_version::detect(command).await.map_err(|err| {
            ProviderError::InvalidFixture(cli_version::version_unavailable(command, &err))
        })?;

        let staged = ticket_attachments::load_for_run(
            input.artifacts_dir.as_deref(),
            input.ticket_id.as_deref(),
        );
        let mut args = cursor_cli_args(
            &worktree,
            input.read_only_tools,
            crate::domain::workflow::is_read_only_job(&input.job_type),
            input.model.as_deref(),
            input.resume_session_id.as_deref(),
            Some(version.as_str()),
            &ticket_attachments::launch_prompt(coppice_run_prompt(), &staged),
        );
        args.extend(ticket_attachments::add_dir_args(&staged));

        // Auth is host-managed: the operator runs `cursor-agent login` wherever the
        // server runs. The child process inherits that environment directly.
        // Coppice does not inject or strip credentials.

        let mut env = Vec::new();
        let _mcp_worktree;
        if let Some(access) = &input.mcp {
            let state_dir = cursor_state_dir(&input)?;
            let (mcp_env, guard) = cursor_mcp_setup(access, &state_dir, &worktree)?;
            env.extend(mcp_env);
            env.extend(access.env().map(|(k, v)| (k.to_string(), v)));
            args.push(APPROVE_MCPS_FLAG.to_string());
            _mcp_worktree = Some(guard);
        } else {
            _mcp_worktree = None;
        }

        log_launch(
            command,
            &version,
            &args,
            run_log_dir(input.artifacts_dir.as_deref(), input.run_id.as_deref()).as_deref(),
        );

        let invocation = CliInvocation {
            program: command.to_string(),
            args,
            env,
            cwd: if contract.process_cwd_is_worktree {
                worktree.clone()
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
        let mut handler = CursorLines {
            stream: input.stream.clone(),
            console: CursorConsolePublisher::new(super::tool_name_style(self.id())),
            assistant_text: String::new(),
            result_error: None,
            saw_result_event: false,
            tools: Default::default(),
        };
        let io = RunIo {
            cancel_rx: input.cancel_rx,
            session_created_tx: input.session_created_tx,
        };

        let CliExit {
            status,
            stderr_tail,
            had_terminal_result,
            stopped_after_result: _,
        } = match run_cli(invocation, &mut handler, io).await {
            Ok(exit) => exit,
            Err(CliError::Spawn(err)) => {
                return Err(ProviderError::Io(std::io::Error::new(
                    err.kind(),
                    format!(
                        "Couldn't start `{command}` in {}: {err}",
                        worktree.display()
                    ),
                )));
            }
            Err(CliError::Io(e)) => return Err(ProviderError::Io(e)),
            Err(CliError::Cancelled) => return Err(ProviderError::Cancelled),
            Err(CliError::Stalled { reason, stderr_tail }) => {
                return Err(super::terminal::stalled("Cursor", &reason, &stderr_tail));
            }
            Err(CliError::TimedOut { stderr_tail }) => {
                return Err(ProviderError::InvalidFixture(format!(
                    "`{command}` timed out after {}s{}",
                    run_timeout.as_secs(),
                    format_stderr_suffix(&stderr_tail)
                )));
            }
        };

        if let Some(msg) = handler.result_error {
            return Err(ProviderError::InvalidFixture(format!(
                "`{command}` result error: {msg}{}",
                format_stderr_suffix(&stderr_tail)
            )));
        }

        // Prefer a successful stream result over a weird non-zero exit.
        // A process that outlives the result is stopped and is not a failure.
        if !status.success() && !handler.saw_result_event && !had_terminal_result {
            return Err(ProviderError::InvalidFixture(format!(
                "`{command}` exited with {status} (cwd {}){}",
                worktree.display(),
                format_stderr_suffix(&stderr_tail)
            )));
        }
        if !status.success() && handler.saw_result_event {
            tracing::warn!(
                command,
                %status,
                stderr = %stderr_tail.join("\n"),
                "cursor CLI returned a result event but exited non-zero; accepting stream result"
            );
        }

        super::terminal::result_from_text(
            &handler.assistant_text,
            input.context_profile,
            "Cursor",
            || {
                format!(
                    "no result contract found in `{command}` output{}",
                    format_stderr_suffix(&stderr_tail)
                )
            },
        )
    }
}

/// Forwards console events and accumulates assistant text; the `result` event
/// is terminal and, unless it is an error, replaces the accumulated text.
struct CursorLines {
    stream: Option<Arc<RunStreamHandle>>,
    console: CursorConsolePublisher,
    assistant_text: String,
    result_error: Option<String>,
    saw_result_event: bool,
    tools: super::cli_runner::ToolTracker,
}

impl LineHandler for CursorLines {
    fn tools_running(&self) -> bool {
        self.tools.any()
    }

    fn on_json(&mut self, value: &serde_json::Value) -> LineStep {
        if value.get("type").and_then(|v| v.as_str()) == Some("tool_call") {
            if let Some(id) = value.get("call_id").and_then(|v| v.as_str()) {
                match value.get("subtype").and_then(|v| v.as_str()) {
                    Some("started") => self.tools.started(id),
                    Some("completed") => self.tools.finished(id),
                    _ => {}
                }
            }
        }
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(stream) = &self.stream {
            if let Some(model) = crate::providers::reported_model::reported_model("cursor", value) {
                stream.note_reported_model(&model);
            }
            self.console.handle_stream_json(stream, value);
        }

        if let Some(text) = extract_assistant_text(value) {
            self.assistant_text.push_str(&text);
        }

        let stop = value.get("type").and_then(|v| v.as_str()) == Some("result");
        if stop {
            self.saw_result_event = true;
            if result_event_is_error(value) {
                self.result_error = Some(
                    value
                        .get("result")
                        .and_then(|v| v.as_str())
                        .unwrap_or("cursor run failed")
                        .to_string(),
                );
            } else if let Some(final_text) = value.get("result").and_then(|v| v.as_str()) {
                self.assistant_text = final_text.to_string();
            }
        }

        LineStep { session_id, stop }
    }
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

/// Without this flag `-p` leaves the workspace `mcp.json` on "needs approval" and
/// never contacts the gateway. `--force` does not imply it.
const APPROVE_MCPS_FLAG: &str = "--approve-mcps";

/// Build `cursor-agent` CLI argv (excluding the binary name).
///
/// Ticket runs use `--force` (write-capable). Plan runs use `--mode plan` and
/// omit `--force`. Chat uses `--mode ask` and omits `--force`. Ask mode blocks
/// edit/apply but was not shown to block shell writes.
fn cursor_cli_args(
    worktree: &Path,
    read_only_tools: bool,
    plan: bool,
    model: Option<&str>,
    resume_session_id: Option<&str>,
    version: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    let contract = coppice_connectors::get(coppice_connectors::CURSOR)
        .expect("cursor descriptor")
        .run_contract;
    let worktree_arg = worktree.display().to_string();
    let pinned = contract.argv(&coppice_connectors::LaunchSubst {
        read_only: read_only_tools,
        plan,
        worktree: &worktree_arg,
        hostname: "",
        port: "",
        version,
    });
    let mut args = contract.with_prompt(pinned, prompt);
    crate::providers::model_flag::push_model_flag(&mut args, "--model", model);
    if let Some(sid) = resume_session_id {
        if !sid.is_empty() {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
    }
    args
}

/// Where this run's `CURSOR_CONFIG_DIR` lives.
///
/// The CLI stores its `chats` state under `CURSOR_CONFIG_DIR`, so a directory
/// keyed by run id would lose every previous turn and `--resume <sid>` would
/// fail. Scope the directory to whatever the resume key is instead:
///
/// - chat turns resume by chat session → `chat-sessions/<chat session id>`
/// - ticket runs resume the newest session for `(ticket, agent)` → `tickets/<ticket id>`
/// - anything else has no resume and stays per run → `runs/<run id>`
///
/// Runs sharing a directory rewrite `cli-config.json` on every turn. The file
/// is byte-identical across runs of the same server (it does not hold the
/// token), so concurrent runs on one ticket cannot corrupt each other.
fn cursor_state_dir(input: &AgentRunInput) -> Result<PathBuf, ProviderError> {
    let missing = || mcp_unavailable("cursor has no run artifacts dir for CURSOR_CONFIG_DIR");
    let artifacts_dir = input.artifacts_dir.as_deref().ok_or_else(missing)?;
    let segments: [&str; 2] = match (&input.chat_session_id, &input.ticket_id, &input.run_id) {
        (Some(chat_session_id), _, _) => ["chat-sessions", chat_session_id],
        (None, Some(ticket_id), _) => ["tickets", ticket_id],
        (None, None, Some(run_id)) => ["runs", run_id],
        (None, None, None) => return Err(missing()),
    };
    Ok(artifacts_path(artifacts_dir, &segments)?)
}

/// Restores `<worktree>/.cursor/mcp.json` when the run ends.
///
/// The CLI has no `--mcp-config`. It reads MCP from the workspace file and
/// `$HOME/.cursor/mcp.json`. Remapping `HOME` hid the operator login (Keychain
/// on macOS). Writing the worktree file and putting it back is the remaining
/// path that keeps auth and still injects the gateway.
struct CursorWorktreeMcp {
    path: PathBuf,
    previous: Option<Vec<u8>>,
    created_dir: bool,
}

impl Drop for CursorWorktreeMcp {
    fn drop(&mut self) {
        match &self.previous {
            Some(raw) => {
                let _ = std::fs::write(&self.path, raw);
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
                if self.created_dir {
                    if let Some(dir) = self.path.parent() {
                        let _ = std::fs::remove_dir(dir);
                    }
                }
            }
        }
    }
}

/// Permissions come from `$CURSOR_CONFIG_DIR/cli-config.json`. MCP servers
/// come from `<worktree>/.cursor/mcp.json`. `HOME` is left alone so the CLI
/// still finds Keychain / `~/.cursor` login.
fn cursor_mcp_setup(
    access: &McpAccess,
    state_dir: &Path,
    worktree: &Path,
) -> std::io::Result<(Vec<(String, String)>, CursorWorktreeMcp)> {
    let config_dir = state_dir.join("cursor-config");
    std::fs::create_dir_all(&config_dir)?;

    let spec = McpServerSpec::from_access(access);
    // Without this allow rule `-p` denies every gateway call at the approval prompt.
    std::fs::write(config_dir.join("cli-config.json"), spec.cursor_cli_config())?;

    let cursor_dir = worktree.join(".cursor");
    let mcp_path = cursor_dir.join("mcp.json");
    let created_dir = !cursor_dir.exists();
    std::fs::create_dir_all(&cursor_dir)?;
    let previous = if mcp_path.is_file() {
        Some(std::fs::read(&mcp_path)?)
    } else {
        None
    };
    let body = match &previous {
        Some(raw) => merge_cursor_mcp_json(raw, &spec),
        None => spec.cursor_mcp_json(),
    };
    std::fs::write(&mcp_path, body)?;
    exclude_worktree_mcp(worktree);

    Ok((
        vec![(
            "CURSOR_CONFIG_DIR".to_string(),
            config_dir.display().to_string(),
        )],
        CursorWorktreeMcp {
            path: mcp_path,
            previous,
            created_dir,
        },
    ))
}

fn merge_cursor_mcp_json(existing: &[u8], spec: &McpServerSpec) -> String {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(existing) else {
        return spec.cursor_mcp_json();
    };
    let Some(servers) = value.get_mut("mcpServers").and_then(|v| v.as_object_mut()) else {
        return spec.cursor_mcp_json();
    };
    let Ok(spec_val) = serde_json::from_str::<serde_json::Value>(&spec.cursor_mcp_json()) else {
        return spec.cursor_mcp_json();
    };
    if let Some(server) = spec_val.get("mcpServers").and_then(|s| s.get("coppice")) {
        servers.insert("coppice".into(), server.clone());
    }
    serde_json::to_string(&value).unwrap_or_else(|_| spec.cursor_mcp_json())
}

fn exclude_worktree_mcp(worktree: &Path) {
    let Ok(out) = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(worktree)
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let rel = String::from_utf8_lossy(&out.stdout);
    let rel = rel.trim();
    if rel.is_empty() {
        return;
    }
    let path = {
        let p = Path::new(rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            worktree.join(p)
        }
    };
    let line = ".cursor/mcp.json";
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(line);
    body.push('\n');
    let _ = std::fs::write(path, body);
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
    use crate::sessions::opencode_events::extract_result_from_text;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/cursor")
    }

    #[test]
    fn chat_turns_use_ask_mode_without_force() {
        let args = cursor_cli_args(
            Path::new("/tmp/chat"),
            true,
            false,
            Some("auto"),
            None,
            None,
            coppice_run_prompt(),
        );
        assert!(args.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(!args.iter().any(|a| a == "--force"));
        assert!(args.windows(2).any(|w| w == ["--model", "auto"]));
    }

    #[test]
    fn blank_model_omits_model_flag_from_launched_command() {
        for model in [None, Some(""), Some("   ")] {
            let args = cursor_cli_args(
                Path::new("/tmp/wt"),
                false,
                false,
                model,
                None,
                None,
                coppice_run_prompt(),
            );
            assert!(
                args.iter().all(|arg| arg != "--model" && arg != "-m"),
                "model {model:?} launched {args:?}"
            );
        }
    }

    #[test]
    fn ticket_turns_use_force_without_ask_mode() {
        let args = cursor_cli_args(
            Path::new("/tmp/wt"),
            false,
            false,
            None,
            Some("sess-1"),
            None,
            coppice_run_prompt(),
        );
        assert!(args.iter().any(|a| a == "--force"));
        assert!(!args.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(args.windows(2).any(|w| w == ["--resume", "sess-1"]));
    }

    #[test]
    fn plan_runs_use_plan_mode_without_force() {
        let args = cursor_cli_args(
            Path::new("/tmp/plan"),
            false,
            true,
            None,
            None,
            None,
            coppice_run_prompt(),
        );
        assert!(args.windows(2).any(|w| w == ["--mode", "plan"]));
        assert!(!args.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(!args.iter().any(|a| a == "--force"));
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

    fn access() -> crate::mcp::grant::McpAccess {
        crate::mcp::grant::McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "super-secret-run-token".into(),
        }
    }

    fn run_input() -> AgentRunInput {
        AgentRunInput {
            agent_id: "agent-1".into(),
            agent_key: "backend_engineer".into(),
            agent_role: "Backend Engineer".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: crate::domain::context_profile::ContextProfile::Full,
            context_path: "/tmp/wt/.agent/context.md".into(),
            run_id: Some("run-1".into()),
            chat_session_id: None,
            artifacts_dir: Some("./data/artifacts".into()),
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

    #[test]
    fn cursor_state_dir_follows_the_resume_key() {
        // Chat turns resume by chat session, so the CLI's `chats` state must
        // outlive the run.
        let mut chat = run_input();
        chat.chat_session_id = Some("chat-9".into());
        chat.ticket_id = Some("ticket-7".into());
        let dir = cursor_state_dir(&chat).expect("chat dir");
        assert!(
            dir.ends_with("data/artifacts/chat-sessions/chat-9"),
            "{dir:?}"
        );

        // Ticket runs resume the newest session for the ticket.
        let mut ticket = run_input();
        ticket.ticket_id = Some("ticket-7".into());
        let dir = cursor_state_dir(&ticket).expect("ticket dir");
        assert!(dir.ends_with("data/artifacts/tickets/ticket-7"), "{dir:?}");

        // Everything else never resumes and stays per run.
        let dir = cursor_state_dir(&run_input()).expect("run dir");
        assert!(dir.ends_with("data/artifacts/runs/run-1"), "{dir:?}");
    }

    #[test]
    fn cursor_state_dir_is_absolute_so_home_is_not_resolved_in_the_worktree() {
        // The CLI is spawned with `current_dir(worktree)`; a relative
        // CURSOR_CONFIG_DIR would land inside the repo checkout.
        for input in [run_input(), {
            let mut chat = run_input();
            chat.chat_session_id = Some("chat-9".into());
            chat
        }] {
            let dir = cursor_state_dir(&input).expect("state dir");
            assert!(dir.is_absolute(), "{dir:?}");
        }
    }

    #[test]
    fn cursor_state_dir_without_artifacts_dir_is_mcp_unavailable() {
        let mut input = run_input();
        input.artifacts_dir = None;
        let err = cursor_state_dir(&input).expect_err("no artifacts dir");
        assert!(err.to_string().contains("mcp_unavailable: cursor"), "{err}");
    }

    #[test]
    fn cursor_mcp_setup_keeps_real_home_and_writes_worktree_mcp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let worktree = tmp.path().join("wt");
        let run_dir = tmp.path().join("runs").join("run-1");
        std::fs::create_dir_all(&worktree).expect("worktree");

        let (env, guard) = cursor_mcp_setup(&access(), &run_dir, &worktree).expect("setup");
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["CURSOR_CONFIG_DIR"]);
        assert_eq!(
            env[0].1,
            run_dir.join("cursor-config").display().to_string()
        );

        let spec = McpServerSpec::from_access(&access());
        let mcp_path = worktree.join(".cursor").join("mcp.json");
        let mcp_raw = std::fs::read_to_string(&mcp_path).expect("mcp");
        assert_eq!(mcp_raw, spec.cursor_mcp_json());
        assert!(!mcp_raw.contains("super-secret-run-token"));

        let cli_raw = std::fs::read_to_string(run_dir.join("cursor-config").join("cli-config.json"))
            .expect("cli config");
        assert_eq!(cli_raw, spec.cursor_cli_config());
        assert!(!run_dir.join("cursor-home").exists());

        drop(guard);
        assert!(!mcp_path.exists(), "created mcp.json must be removed");
    }

    #[test]
    fn cursor_mcp_setup_merges_and_restores_existing_worktree_mcp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let worktree = tmp.path().join("wt");
        let cursor_dir = worktree.join(".cursor");
        std::fs::create_dir_all(&cursor_dir).expect("cursor dir");
        let mcp_path = cursor_dir.join("mcp.json");
        let existing = r#"{"mcpServers":{"figma":{"url":"http://127.0.0.1:9/mcp"}}}"#;
        std::fs::write(&mcp_path, existing).expect("existing mcp");

        let (env, guard) =
            cursor_mcp_setup(&access(), &tmp.path().join("run"), &worktree).expect("setup");
        assert!(env.iter().all(|(k, _)| k != "HOME"));

        let merged: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp_path).expect("read")).expect("json");
        assert!(merged["mcpServers"]["figma"].is_object(), "{merged}");
        assert_eq!(
            merged["mcpServers"]["coppice"]["url"],
            "http://127.0.0.1:5000/mcp"
        );
        assert!(!merged.to_string().contains("super-secret-run-token"));

        drop(guard);
        assert_eq!(std::fs::read_to_string(&mcp_path).expect("restored"), existing);
        assert!(cursor_dir.is_dir(), "pre-existing .cursor must stay");
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
        assert_eq!(captured_id.as_deref(), Some("sess_cursor_abc"));
    }

    #[test]
    fn error_result_is_rejected() {
        let raw =
            std::fs::read_to_string(fixtures_root().join("error.jsonl")).expect("read error.jsonl");
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
        let mut console = CursorConsolePublisher::new(crate::providers::ToolNameStyle::Dash);
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
        assert_eq!(events.len(), 4, "session + 2 text + result");
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
