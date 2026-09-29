use super::cursor_console::CursorConsolePublisher;
use super::{
    artifacts_path, mcp_unavailable, worktree_dir_from_context, AgentProvider, AgentRunInput,
    AgentRunResult, ProviderError,
};
use crate::mcp::grant::McpAccess;
use crate::sessions::opencode_events::{coppice_run_prompt, extract_result_from_text};
use async_trait::async_trait;
use coppice_config::CursorProviderConfig;
use std::path::{Path, PathBuf};
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
        let worktree = worktree_dir_from_context(&input.context_path)?;

        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        let command = self.config.command.as_str();

        let mut cmd = Command::new(command);
        for arg in cursor_cli_args(
            &worktree,
            input.read_only_tools,
            input.model.as_deref(),
            input.resume_session_id.as_deref(),
        ) {
            cmd.arg(arg);
        }
        cmd.current_dir(&worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Auth is host-managed: the operator runs `agent login` wherever the
        // server runs. The child process inherits that environment directly.
        // Coppice does not inject or strip credentials.

        if let Some(access) = &input.mcp {
            let state_dir = cursor_state_dir(&input)?;
            cmd.envs(cursor_mcp_setup(access, &state_dir, &HostEnv::from_process())?);
            cmd.envs(access.env());
        }

        tracing::info!(
            command,
            cwd = %worktree.display(),
            model = input.model.as_deref().unwrap_or(""),
            resume = input.resume_session_id.as_deref().unwrap_or(""),
            read_only_tools = input.read_only_tools,
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
            ProviderError::MissingResult(format!(
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

/// Build `agent` CLI argv (excluding the binary name).
///
/// Ticket runs use `--force` (write-capable). Chat turns use `--mode ask`
/// (Cursor's read-only Q&A mode) and omit `--force`.
fn cursor_cli_args(
    worktree: &Path,
    read_only_tools: bool,
    model: Option<&str>,
    resume_session_id: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        coppice_run_prompt().to_string(),
        "--trust".to_string(),
    ];
    if read_only_tools {
        args.push("--mode".to_string());
        args.push("ask".to_string());
    } else {
        args.push("--force".to_string());
    }
    args.extend([
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--workspace".to_string(),
        worktree.display().to_string(),
    ]);
    if let Some(model) = model {
        if !model.is_empty() {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
    }
    if let Some(sid) = resume_session_id {
        if !sid.is_empty() {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
    }
    args
}

/// The host values the per-run environment is derived from, captured before
/// `HOME` is overridden.
struct HostEnv {
    home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    git_config_global: Option<String>,
    gh_config_dir: Option<String>,
}

impl HostEnv {
    fn from_process() -> Self {
        Self {
            home: std::env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            git_config_global: std::env::var("GIT_CONFIG_GLOBAL").ok(),
            gh_config_dir: std::env::var("GH_CONFIG_DIR").ok(),
        }
    }

    /// Where the CLI keeps `cursor/auth.json`. `HOME` no longer points at it
    /// once the run gets its own home, so it must be passed explicitly.
    fn config_home(&self) -> Option<PathBuf> {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home.as_ref().map(|home| home.join(".config")))
    }
}

/// `<run home>/.cursor/mcp.json`. Serialized from structs so the file matches
/// the verified layout field for field.
#[derive(serde::Serialize)]
struct McpFile<'a> {
    #[serde(rename = "mcpServers")]
    mcp_servers: McpServers<'a>,
}

#[derive(serde::Serialize)]
struct McpServers<'a> {
    coppice: McpServer<'a>,
}

#[derive(serde::Serialize)]
struct McpServer<'a> {
    url: &'a str,
    headers: McpHeaders,
}

#[derive(serde::Serialize)]
struct McpHeaders {
    #[serde(rename = "Authorization")]
    authorization: &'static str,
}

impl<'a> McpFile<'a> {
    fn for_gateway(url: &'a str) -> Self {
        Self {
            mcp_servers: McpServers {
                coppice: McpServer {
                    url,
                    headers: McpHeaders {
                        authorization: "Bearer ${env:COPPICE_MCP_TOKEN}",
                    },
                },
            },
        }
    }
}

/// `<run config dir>/cli-config.json`.
#[derive(serde::Serialize)]
struct CliConfigFile {
    version: u32,
    permissions: CliPermissions,
}

#[derive(serde::Serialize)]
struct CliPermissions {
    allow: Vec<&'static str>,
    deny: Vec<&'static str>,
}

impl CliConfigFile {
    fn allowing_gateway() -> Self {
        Self {
            version: 1,
            permissions: CliPermissions {
                allow: vec!["Mcp(coppice:*)"],
                deny: Vec::new(),
            },
        }
    }
}

/// Where this run's `HOME` and `CURSOR_CONFIG_DIR` live.
///
/// The CLI stores its `chats` state under `CURSOR_CONFIG_DIR`, so a directory
/// keyed by run id would lose every previous turn and `--resume <sid>` would
/// fail. Scope the directory to whatever the resume key is instead:
///
/// - chat turns resume by chat session → `chat-sessions/<chat session id>`
/// - ticket runs resume the newest session for `(ticket, agent)` → `tickets/<ticket id>`
/// - anything else has no resume and stays per run → `runs/<run id>`
///
/// Runs sharing a directory rewrite `mcp.json` and `cli-config.json` on every
/// turn. Both are byte-identical across runs of the same server (neither holds
/// the token), so concurrent runs on one ticket cannot corrupt each other.
fn cursor_state_dir(input: &AgentRunInput) -> Result<PathBuf, ProviderError> {
    let missing =
        || mcp_unavailable("cursor has no run artifacts dir for its per-run HOME");
    let artifacts_dir = input.artifacts_dir.as_deref().ok_or_else(missing)?;
    let segments: [&str; 2] = match (&input.chat_session_id, &input.ticket_id, &input.run_id) {
        (Some(chat_session_id), _, _) => ["chat-sessions", chat_session_id],
        (None, Some(ticket_id), _) => ["tickets", ticket_id],
        (None, None, Some(run_id)) => ["runs", run_id],
        (None, None, None) => return Err(missing()),
    };
    Ok(artifacts_path(artifacts_dir, &segments)?)
}

/// Verified mechanism (plan task 1): the CLI reads MCP servers only from the
/// workspace and from `$HOME/.cursor/mcp.json`, and permissions only from
/// `$CURSOR_CONFIG_DIR/cli-config.json`. Give the run its own `HOME` and config
/// dir under the artifacts dir so nothing is written to the workspace or the
/// operator's real `~/.cursor`.
fn cursor_mcp_setup(
    access: &McpAccess,
    state_dir: &Path,
    host: &HostEnv,
) -> std::io::Result<Vec<(String, String)>> {
    let home = state_dir.join("cursor-home");
    let config_dir = state_dir.join("cursor-config");
    std::fs::create_dir_all(home.join(".cursor"))?;
    std::fs::create_dir_all(&config_dir)?;

    // `${env:…}` is interpolated by the CLI, so the token never hits the file.
    std::fs::write(
        home.join(".cursor").join("mcp.json"),
        serde_json::to_string(&McpFile::for_gateway(&access.url))?,
    )?;
    // Without this allow rule `-p` denies every gateway call at the approval prompt.
    std::fs::write(
        config_dir.join("cli-config.json"),
        serde_json::to_string(&CliConfigFile::allowing_gateway())?,
    )?;

    let mut env = vec![
        ("HOME".to_string(), home.display().to_string()),
        (
            "CURSOR_CONFIG_DIR".to_string(),
            config_dir.display().to_string(),
        ),
    ];
    let host_config_home = host.config_home();
    if let Some(config_home) = &host_config_home {
        env.push((
            "XDG_CONFIG_HOME".to_string(),
            config_home.display().to_string(),
        ));
    }
    // The agent's own shell commands lose the real `HOME`; forward the git and
    // gh config the run still needs, unless the operator already set them.
    if host.git_config_global.is_none() {
        if let Some(gitconfig) = host.home.as_ref().map(|home| home.join(".gitconfig")) {
            if gitconfig.is_file() {
                env.push((
                    "GIT_CONFIG_GLOBAL".to_string(),
                    gitconfig.display().to_string(),
                ));
            }
        }
    }
    if host.gh_config_dir.is_none() {
        if let Some(gh) = host_config_home.map(|config_home| config_home.join("gh")) {
            if gh.is_dir() {
                env.push(("GH_CONFIG_DIR".to_string(), gh.display().to_string()));
            }
        }
    }
    Ok(env)
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
    fn chat_turns_use_ask_mode_without_force() {
        let args = cursor_cli_args(Path::new("/tmp/chat"), true, Some("auto"), None);
        assert!(args.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(!args.iter().any(|a| a == "--force"));
        assert!(args.windows(2).any(|w| w == ["--model", "auto"]));
    }

    #[test]
    fn ticket_turns_use_force_without_ask_mode() {
        let args = cursor_cli_args(Path::new("/tmp/wt"), false, None, Some("sess-1"));
        assert!(args.iter().any(|a| a == "--force"));
        assert!(!args.windows(2).any(|w| w == ["--mode", "ask"]));
        assert!(args.windows(2).any(|w| w == ["--resume", "sess-1"]));
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
        assert!(dir.ends_with("data/artifacts/chat-sessions/chat-9"), "{dir:?}");

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
        // The CLI is spawned with `current_dir(worktree)`; a relative HOME
        // would land inside the repo checkout.
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
    fn cursor_mcp_setup_writes_per_run_home_and_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real_home = tmp.path().join("real-home");
        std::fs::create_dir_all(real_home.join(".config").join("gh")).expect("gh dir");
        std::fs::write(real_home.join(".gitconfig"), "[user]\n").expect("gitconfig");
        let run_dir = tmp.path().join("runs").join("run-1");

        let host = HostEnv {
            home: Some(real_home.clone()),
            xdg_config_home: None,
            git_config_global: None,
            gh_config_dir: None,
        };
        let env = cursor_mcp_setup(&access(), &run_dir, &host).expect("setup");
        let lookup = |key: &str| {
            env.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("missing {key}"))
        };

        let home = run_dir.join("cursor-home");
        let config = run_dir.join("cursor-config");
        assert_eq!(lookup("HOME"), home.display().to_string());
        assert_eq!(lookup("CURSOR_CONFIG_DIR"), config.display().to_string());
        assert_eq!(
            lookup("XDG_CONFIG_HOME"),
            real_home.join(".config").display().to_string()
        );
        assert_eq!(
            lookup("GIT_CONFIG_GLOBAL"),
            real_home.join(".gitconfig").display().to_string()
        );
        assert_eq!(
            lookup("GH_CONFIG_DIR"),
            real_home.join(".config").join("gh").display().to_string()
        );

        let mcp_raw = std::fs::read_to_string(home.join(".cursor").join("mcp.json")).expect("mcp");
        assert_eq!(
            mcp_raw,
            r#"{"mcpServers":{"coppice":{"url":"http://127.0.0.1:5000/mcp","headers":{"Authorization":"Bearer ${env:COPPICE_MCP_TOKEN}"}}}}"#
        );
        assert!(!mcp_raw.contains("super-secret-run-token"));

        let cli_raw = std::fs::read_to_string(config.join("cli-config.json")).expect("cli config");
        assert_eq!(
            cli_raw,
            r#"{"version":1,"permissions":{"allow":["Mcp(coppice:*)"],"deny":[]}}"#
        );
    }

    #[test]
    fn cursor_mcp_setup_keeps_existing_host_config_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real_home = tmp.path().join("real-home");
        let real_xdg = tmp.path().join("xdg");
        std::fs::create_dir_all(real_xdg.join("gh")).expect("gh dir");
        std::fs::create_dir_all(&real_home).expect("home");
        let run_dir = tmp.path().join("runs").join("run-2");

        let host = HostEnv {
            home: Some(real_home),
            xdg_config_home: Some(real_xdg.clone()),
            git_config_global: Some("/etc/gitconfig".into()),
            gh_config_dir: Some("/etc/gh".into()),
        };
        let env = cursor_mcp_setup(&access(), &run_dir, &host).expect("setup");
        let lookup = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

        assert_eq!(lookup("XDG_CONFIG_HOME"), Some(real_xdg.display().to_string().as_str()));
        // Already-set host values win; the connector does not override them.
        assert_eq!(lookup("GIT_CONFIG_GLOBAL"), None);
        assert_eq!(lookup("GH_CONFIG_DIR"), None);
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
