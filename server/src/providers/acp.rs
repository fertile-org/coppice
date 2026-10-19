//! Agent Client Protocol (ACP) over stdio: JSON-RPC 2.0, one message per line.
//!
//! Coppice is the client. One process runs one turn: `initialize`, then
//! `session/new` (or `session/resume` / `session/load`), an optional model
//! choice, and `session/prompt`, which returns when the turn ends. While the
//! prompt runs the agent streams `session/update` notifications and may ask
//! for permission before a tool runs.
//!
//! Behaviour this file relies on, checked against `opencode acp` 2.0.20:
//! - `initialize` reports `agentCapabilities.mcpCapabilities.http`, so the
//!   Coppice gateway is passed in `session/new` with its bearer header. The
//!   token never touches disk.
//! - The model is a `configOptions` entry (`category: "model"`), set with
//!   `session/set_config_option`.
//! - Refusing a permission request ends the whole turn with
//!   `stopReason: "cancelled"`; the agent cannot carry on. Read-only runs
//!   therefore deny tools in the agent's own config, so it never asks, and
//!   the refusal here is only a second layer.
//! - MCP tool calls are not put through a permission request.

use super::line_reader::{CappedLines, MAX_LINE_BYTES};
use super::ProviderError;
use crate::mcp::grant::McpAccess;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio::time::Instant;

pub const PROTOCOL_VERSION: u64 = 1;

/// How long a CLI gets to answer each startup request.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);
/// A turn with no update for this long, and no tool still running, has stalled.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// After `session/cancel`, how long to wait for the turn to end on its own.
const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// After the turn, how long the process gets to exit once stdin closes.
const EXIT_GRACE: Duration = Duration::from_secs(3);

pub struct AcpLaunch {
    /// Shown in errors, e.g. "OpenCode".
    pub display_name: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub run_id: Option<String>,
    pub ticket_id: Option<String>,
    pub task_log_dir: Option<PathBuf>,
    /// How to sign in, for an agent that answers "Authentication required".
    pub sign_in_hint: Option<String>,
}

/// Which tool calls Coppice approves when the agent asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// Ticket runs: approve every request, like a write run's bypass flag.
    AllowAll,
    /// Chat, planning, checks: approve only tools that read.
    ReadOnly,
}

pub struct AcpTurn<'a> {
    pub cwd: &'a Path,
    pub mcp: Option<&'a McpAccess>,
    /// A model `configOptions` value, such as `opencode/big-pickle`.
    pub model: Option<&'a str>,
    pub resume_session_id: Option<&'a str>,
    pub prompt: &'a str,
    pub policy: ToolPolicy,
    /// A `mode` session option value to set, such as Kimi Code's `yolo`.
    /// Skipped when the agent offers no such option.
    pub session_mode: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct AcpLimits {
    pub handshake: Duration,
    pub run: Duration,
    pub idle: Duration,
}

impl AcpLimits {
    pub fn for_run(run: Duration) -> Self {
        Self {
            handshake: HANDSHAKE_TIMEOUT,
            run,
            idle: IDLE_TIMEOUT,
        }
    }
}

/// Receives the session and every streamed update, in order.
pub trait AcpObserver: Send {
    fn on_session(&mut self, session_id: &str, model: Option<&str>);
    fn on_update(&mut self, update: &Value);
    /// The turn is over; flush anything buffered.
    fn on_end(&mut self);
}

#[derive(Debug)]
pub struct AcpOutcome {
    pub session_id: String,
    /// `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, or `cancelled`.
    pub stop_reason: String,
    /// The agent's message text, in order.
    pub agent_text: String,
    /// Titles of tool calls Coppice refused.
    pub refused: Vec<String>,
    pub stderr_tail: Vec<String>,
    /// `(value, name)` of the models the session offered.
    pub models: Vec<(String, String)>,
}

impl AcpOutcome {
    /// Why the turn ended without finishing, in words, or `None` for `end_turn`.
    pub fn stop_problem(&self, display_name: &str) -> Option<String> {
        match self.stop_reason.as_str() {
            "end_turn" => None,
            "max_tokens" => Some(format!("{display_name} hit its output token limit before finishing")),
            "max_turn_requests" => Some(format!("{display_name} hit its limit on model requests for one turn")),
            "refusal" => Some(format!("{display_name}'s model refused to continue")),
            "cancelled" if !self.refused.is_empty() => Some(format!(
                "{display_name} stopped the turn after Coppice refused: {}. This run only allows tools that read.",
                self.refused.join("; ")
            )),
            "cancelled" => Some(format!("{display_name} cancelled the turn")),
            other => Some(format!("{display_name} ended the turn with stop reason `{other}`")),
        }
    }
}

/// Runs one turn and stops the process.
pub async fn run_turn(
    launch: AcpLaunch,
    turn: AcpTurn<'_>,
    limits: AcpLimits,
    observer: &mut dyn AcpObserver,
    cancel_rx: Option<watch::Receiver<bool>>,
    session_tx: Option<watch::Sender<String>>,
) -> Result<AcpOutcome, ProviderError> {
    let host = crate::host_env::current();
    let program = super::cli_runner::resolve_cli_program(&host, &launch.program)
        .map_err(|err| ProviderError::Io(std::io::Error::other(err.to_string())))?;
    let mut secret_values: Vec<(String, String)> = launch.env.clone();
    if let Some(access) = turn.mcp {
        secret_values.push(("COPPICE_MCP_TOKEN".into(), access.token.clone()));
    }
    let secrets = crate::logs::guard_for_child_env(
        secret_values.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    );

    let mut cmd = Command::new(&program);
    cmd.env("PATH", &host.path);
    if !host.home.as_os_str().is_empty() {
        cmd.env("HOME", &host.home);
    }
    super::env_hygiene::scrub(&mut cmd);
    cmd.args(&launch.args)
        .envs(launch.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(&launch.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tracked = crate::process_tree::active()
        .spawn(
            &mut cmd,
            crate::process_tree::SpawnMeta {
                command: launch.program.clone(),
                label: "acp",
                run_id: launch.run_id.clone(),
                task_log_dir: launch.task_log_dir.clone(),
            },
        )
        .map_err(|err| {
            ProviderError::Io(std::io::Error::other(format!(
                "Couldn't start {}: {err}",
                launch.display_name
            )))
        })?;

    let stdin = tracked.take_stdin().expect("piped stdin");
    let stdout = tracked.take_stdout().expect("piped stdout");
    let stderr = tracked.take_stderr().expect("piped stderr");
    let stderr_tail = Arc::new(Mutex::new(Vec::new()));
    let stderr_task = tokio::spawn(super::cli_runner::pump_stderr(
        stderr,
        secrets.clone(),
        launch.run_id.clone(),
        launch.ticket_id.clone(),
        stderr_tail.clone(),
    ));

    let mut conn = Connection {
        name: launch.display_name.clone(),
        sign_in_hint: launch.sign_in_hint.clone(),
        stdin: Some(stdin),
        lines: CappedLines::new(BufReader::new(stdout), MAX_LINE_BYTES),
        next_id: 0,
        policy: turn.policy,
        observer,
        cancel_rx,
        cancel_sent: false,
        session_id: None,
        replaying: false,
        tools_in_flight: HashSet::new(),
        last_activity: Instant::now(),
        agent_text: String::new(),
        refused: Vec::new(),
        run_deadline: Instant::now() + limits.run,
        limits,
    };

    let result = conn.drive(&turn, session_tx).await;
    let stderr_lines = || std::mem::take(&mut *stderr_tail.lock().unwrap());

    // Closing stdin is how an ACP agent learns the client is done.
    conn.stdin.take();
    let exited = tokio::time::timeout(EXIT_GRACE, tracked.wait_and_reap("turn ended")).await;
    if exited.is_err() {
        let _ = tracked.shutdown("did not exit after the turn").await;
    }
    if tokio::time::timeout(Duration::from_secs(2), stderr_task)
        .await
        .is_err()
    {
        tracing::debug!("acp stderr still open after the process stopped");
    }
    drop(secrets);

    match result {
        Ok(mut outcome) => {
            outcome.stderr_tail = stderr_lines();
            Ok(outcome)
        }
        Err(err) => Err(with_stderr(err, &stderr_lines())),
    }
}

/// Adds the CLI's stderr to a failure, which is often the only place the cause is.
fn with_stderr(err: ProviderError, stderr: &[String]) -> ProviderError {
    let tail: Vec<&str> = stderr
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    if tail.is_empty() {
        return err;
    }
    let suffix = format!(
        " (stderr: {})",
        crate::util::truncate::truncate_with_ellipsis(&tail.join(" | "), 600)
    );
    match err {
        ProviderError::InvalidFixture(msg) => ProviderError::InvalidFixture(msg + &suffix),
        ProviderError::MissingResult(msg) => ProviderError::MissingResult(msg + &suffix),
        ProviderError::InvalidInput(msg) => ProviderError::InvalidInput(msg + &suffix),
        ProviderError::Io(err) => ProviderError::InvalidFixture(format!("{err}{suffix}")),
        other => other,
    }
}

enum Incoming {
    Response(Result<Value, RpcError>),
    Other,
}

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn method_not_found(&self) -> bool {
        self.code == -32601
    }
}

enum Wait {
    Handshake(&'static str),
    Prompt,
}

struct Connection<'o> {
    name: String,
    sign_in_hint: Option<String>,
    stdin: Option<ChildStdin>,
    lines: CappedLines<BufReader<ChildStdout>>,
    next_id: u64,
    policy: ToolPolicy,
    observer: &'o mut dyn AcpObserver,
    cancel_rx: Option<watch::Receiver<bool>>,
    cancel_sent: bool,
    session_id: Option<String>,
    /// `session/load` replays the old conversation as updates; it is not news.
    replaying: bool,
    tools_in_flight: HashSet<String>,
    last_activity: Instant,
    agent_text: String,
    refused: Vec<String>,
    run_deadline: Instant,
    limits: AcpLimits,
}

impl Connection<'_> {
    async fn drive(
        &mut self,
        turn: &AcpTurn<'_>,
        session_tx: Option<watch::Sender<String>>,
    ) -> Result<AcpOutcome, ProviderError> {
        // A read-only run offers to handle file writes and commands, so an
        // agent that routes them to the client (Kimi Code does) asks Coppice,
        // which refuses. Reads stay with the agent.
        let gate = self.policy == ToolPolicy::ReadOnly;
        let init = self
            .call(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "clientCapabilities": {
                        "fs": { "readTextFile": false, "writeTextFile": gate },
                        "terminal": gate
                    }
                }),
                Wait::Handshake("initialize"),
            )
            .await?
            .map_err(|err| self.rpc_failure("initialize", err))?;
        let version = init.get("protocolVersion").and_then(Value::as_u64);
        if version != Some(PROTOCOL_VERSION) {
            return Err(ProviderError::InvalidFixture(format!(
                "{} speaks ACP version {}, and Coppice speaks version {PROTOCOL_VERSION}",
                self.name,
                version.map_or_else(|| "unknown".to_string(), |v| v.to_string())
            )));
        }
        let caps = init.get("agentCapabilities").cloned().unwrap_or(json!({}));
        let mcp_servers = match turn.mcp {
            Some(access) => {
                let http = caps
                    .pointer("/mcpCapabilities/http")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !http {
                    return Err(super::mcp_unavailable(&format!(
                        "{} does not accept HTTP MCP servers over ACP, so it can't reach the Coppice gateway",
                        self.name
                    )));
                }
                json!([gateway_server(access)])
            }
            None => json!([]),
        };

        let cwd = turn.cwd.to_string_lossy().to_string();
        let session = match turn.resume_session_id.filter(|id| !id.is_empty()) {
            Some(id) => self.resume(&caps, id, &cwd, &mcp_servers).await?,
            None => self
                .call(
                    "session/new",
                    json!({ "cwd": cwd, "mcpServers": mcp_servers }),
                    Wait::Handshake("session/new"),
                )
                .await?
                .map_err(|err| self.rpc_failure("session/new", err))?,
        };
        let session_id = match turn.resume_session_id.filter(|id| !id.is_empty()) {
            Some(id) => session
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string(),
            None => session
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| {
                    ProviderError::InvalidFixture(format!(
                        "{} answered session/new without a session id",
                        self.name
                    ))
                })?,
        };
        self.session_id = Some(session_id.clone());
        let models = offered_models(&session);
        if turn.prompt.is_empty() {
            // A look at the session only, such as listing models.
            return Ok(AcpOutcome {
                session_id,
                stop_reason: "no_prompt".into(),
                agent_text: String::new(),
                refused: Vec::new(),
                stderr_tail: Vec::new(),
                models,
            });
        }
        if let Some(tx) = session_tx {
            let _ = tx.send(session_id.clone());
        }

        let model = self
            .choose_model(&session_id, session.get("configOptions"), turn.model)
            .await?;
        if let Some(mode) = turn.session_mode {
            self.choose_mode(&session_id, session.get("configOptions"), mode)
                .await?;
        }
        self.observer.on_session(&session_id, model.as_deref());

        self.last_activity = Instant::now();
        let prompt = self
            .call(
                "session/prompt",
                json!({
                    "sessionId": session_id,
                    "prompt": [{ "type": "text", "text": turn.prompt }]
                }),
                Wait::Prompt,
            )
            .await;
        self.observer.on_end();
        let prompt = prompt?.map_err(|err| self.rpc_failure("session/prompt", err))?;
        if self.cancel_sent {
            return Err(ProviderError::Cancelled);
        }
        let stop_reason = prompt
            .get("stopReason")
            .and_then(Value::as_str)
            .unwrap_or("end_turn")
            .to_string();
        Ok(AcpOutcome {
            session_id,
            stop_reason,
            agent_text: std::mem::take(&mut self.agent_text),
            refused: std::mem::take(&mut self.refused),
            stderr_tail: Vec::new(),
            models,
        })
    }

    async fn resume(
        &mut self,
        caps: &Value,
        session_id: &str,
        cwd: &str,
        mcp_servers: &Value,
    ) -> Result<Value, ProviderError> {
        let params = json!({ "sessionId": session_id, "cwd": cwd, "mcpServers": mcp_servers });
        let method = if caps.pointer("/sessionCapabilities/resume").is_some() {
            "session/resume"
        } else if caps.get("loadSession").and_then(Value::as_bool) == Some(true) {
            self.replaying = true;
            "session/load"
        } else {
            return Err(ProviderError::ResumeSessionInvalid(format!(
                "{} can't resume sessions over ACP",
                self.name
            )));
        };
        let answer = self
            .call(method, params, Wait::Handshake("session resume"))
            .await;
        self.replaying = false;
        match answer? {
            Ok(value) => Ok(value),
            Err(err) if super::resume::session_is_unusable(&err.message) => Err(
                ProviderError::ResumeSessionInvalid(format!("{}: {}", self.name, err.message)),
            ),
            // Not proof the session is gone: an unreachable MCP server or a
            // sign-in problem fails a resume too, and starting over cures neither.
            Err(err) => Err(self.rpc_failure(method, err)),
        }
    }

    /// Selects `wanted` when given. Returns the model in use, if reported.
    async fn choose_model(
        &mut self,
        session_id: &str,
        config_options: Option<&Value>,
        wanted: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let model_option = config_options
            .and_then(Value::as_array)
            .and_then(|options| {
                options.iter().find(|option| {
                    option.get("category").and_then(Value::as_str) == Some("model")
                        || option.get("id").and_then(Value::as_str) == Some("model")
                })
            })
            .cloned();
        let current = model_option
            .as_ref()
            .and_then(|option| option.get("currentValue"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(wanted) = wanted.map(str::trim).filter(|m| !m.is_empty()) else {
            return Ok(current);
        };
        if current.as_deref() == Some(wanted) {
            return Ok(current);
        }
        let Some(option) = model_option else {
            // No config option: the older model call, if the agent has it.
            let answer = self
                .call(
                    "session/set_model",
                    json!({ "sessionId": session_id, "modelId": wanted }),
                    Wait::Handshake("session/set_model"),
                )
                .await?;
            return match answer {
                Ok(_) => Ok(Some(wanted.to_string())),
                Err(err) if err.method_not_found() => Err(ProviderError::InvalidInput(format!(
                    "{} doesn't let Coppice choose a model over ACP. Clear the agent's model to use {}'s default.",
                    self.name, self.name
                ))),
                Err(err) => Err(self.rpc_failure("session/set_model", err)),
            };
        };
        let values = option_values(option.get("options"));
        if !values.is_empty() && !values.iter().any(|value| value == wanted) {
            return Err(ProviderError::InvalidInput(format!(
                "{} doesn't offer the model `{wanted}`. Pick another model for this agent, or click Refresh models.",
                self.name
            )));
        }
        let config_id = option
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("model")
            .to_string();
        self.call(
            "session/set_config_option",
            json!({ "sessionId": session_id, "configId": config_id, "value": wanted }),
            Wait::Handshake("session/set_config_option"),
        )
        .await?
        .map_err(|err| self.rpc_failure("session/set_config_option", err))?;
        Ok(Some(wanted.to_string()))
    }

    /// Sets the `mode` session option when the agent offers `wanted`.
    async fn choose_mode(
        &mut self,
        session_id: &str,
        config_options: Option<&Value>,
        wanted: &str,
    ) -> Result<(), ProviderError> {
        let Some(option) = config_options
            .and_then(Value::as_array)
            .and_then(|options| {
                options
                    .iter()
                    .find(|o| o.get("category").and_then(Value::as_str) == Some("mode"))
            })
            .cloned()
        else {
            tracing::warn!(agent = %self.name, mode = wanted, "the agent offers no mode option");
            return Ok(());
        };
        if option.get("currentValue").and_then(Value::as_str) == Some(wanted) {
            return Ok(());
        }
        if !option_values(option.get("options")).iter().any(|v| v == wanted) {
            tracing::warn!(agent = %self.name, mode = wanted, "the agent has no such mode");
            return Ok(());
        }
        let config_id = option.get("id").and_then(Value::as_str).unwrap_or("mode").to_string();
        self.call(
            "session/set_config_option",
            json!({ "sessionId": session_id, "configId": config_id, "value": wanted }),
            Wait::Handshake("session/set_config_option"),
        )
        .await?
        .map_err(|err| self.rpc_failure("session/set_config_option", err))?;
        Ok(())
    }

    fn rpc_failure(&self, method: &str, err: RpcError) -> ProviderError {
        if err.code == AUTH_REQUIRED
            || err
                .message
                .to_lowercase()
                .contains("authentication required")
        {
            return ProviderError::InvalidFixture(format!(
                "{} isn't signed in. {}",
                self.name,
                self.sign_in_hint
                    .as_deref()
                    .unwrap_or("Sign in with its CLI, then try again.")
            ));
        }
        ProviderError::InvalidFixture(format!(
            "{} rejected {method}: {} (code {})",
            self.name, err.message, err.code
        ))
    }

    async fn send(&mut self, message: &Value) -> Result<(), ProviderError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(ProviderError::InvalidFixture(format!(
                "{} input is closed",
                self.name
            )));
        };
        let mut line = message.to_string();
        line.push('\n');
        let written = async {
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await
        };
        // A stuck writer would hang the turn; the agent reads stdin while it works.
        match tokio::time::timeout(Duration::from_secs(30), written).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                Err(ProviderError::InvalidFixture(format!(
                    "{} exited before reading its input",
                    self.name
                )))
            }
            Ok(Err(err)) => Err(ProviderError::InvalidFixture(format!(
                "write to {}: {err}",
                self.name
            ))),
            Err(_) => Err(ProviderError::InvalidFixture(format!(
                "{} stopped reading its input",
                self.name
            ))),
        }
    }

    /// Sends a request and serves everything else until its answer arrives.
    /// The outer error ends the turn; the inner one is the agent's RPC error.
    async fn call(
        &mut self,
        method: &str,
        params: Value,
        wait: Wait,
    ) -> Result<Result<Value, RpcError>, ProviderError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        let handshake_deadline = match wait {
            Wait::Handshake(_) => Some(Instant::now() + self.limits.handshake),
            Wait::Prompt => None,
        };
        let mut cancel_deadline: Option<Instant> = None;

        loop {
            if self.cancelled() && !self.cancel_sent {
                match (&wait, &self.session_id) {
                    (Wait::Prompt, Some(session_id)) => {
                        let session_id = session_id.clone();
                        self.cancel_sent = true;
                        cancel_deadline = Some(Instant::now() + CANCEL_GRACE);
                        let _ = self
                            .send(&json!({
                                "jsonrpc": "2.0",
                                "method": "session/cancel",
                                "params": { "sessionId": session_id }
                            }))
                            .await;
                    }
                    _ => return Err(ProviderError::Cancelled),
                }
            }

            let now = Instant::now();
            let mut deadline = self.run_deadline;
            if let Some(at) = handshake_deadline {
                deadline = deadline.min(at);
            }
            if let Some(at) = cancel_deadline {
                deadline = deadline.min(at);
            }
            let idle_at = self.last_activity + self.limits.idle;
            let watch_idle = matches!(wait, Wait::Prompt) && self.tools_in_flight.is_empty();
            if watch_idle {
                deadline = deadline.min(idle_at);
            }
            if now >= deadline {
                return Err(self.deadline_error(&wait, cancel_deadline, now, watch_idle, idle_at));
            }

            let mut cancel_rx = self.cancel_rx.clone();
            let line = tokio::select! {
                biased;
                _ = wait_cancel(&mut cancel_rx), if !self.cancel_sent && self.cancel_rx.is_some() => continue,
                _ = tokio::time::sleep_until(deadline) => continue,
                line = self.lines.next_line() => line,
            };
            let line = match line {
                Ok(Some(line)) => line,
                Ok(None) => {
                    return Err(ProviderError::InvalidFixture(format!(
                        "{} exited before answering {method}",
                        self.name
                    )))
                }
                Err(err) => return Err(ProviderError::Io(err)),
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                // Not protocol; some agents log to stdout. Keep it with the run.
                crate::logs::log_cli_line(None, None, "stdout", &line);
                continue;
            };
            if let Incoming::Response(answer) = self.handle(message, id).await? {
                return Ok(answer);
            }
        }
    }

    fn deadline_error(
        &self,
        wait: &Wait,
        cancel_deadline: Option<Instant>,
        now: Instant,
        watch_idle: bool,
        idle_at: Instant,
    ) -> ProviderError {
        if cancel_deadline.is_some_and(|at| now >= at) {
            return ProviderError::Cancelled;
        }
        if now >= self.run_deadline {
            return ProviderError::InvalidFixture(format!(
                "{} run timed out after {}s",
                self.name,
                self.limits.run.as_secs()
            ));
        }
        if watch_idle && now >= idle_at {
            return ProviderError::InvalidFixture(format!(
                "{} sent nothing for {} minutes, so Coppice stopped the run",
                self.name,
                self.limits.idle.as_secs() / 60
            ));
        }
        match wait {
            Wait::Handshake(step) => ProviderError::InvalidFixture(format!(
                "{} didn't answer {step} within {}s",
                self.name,
                self.limits.handshake.as_secs()
            )),
            Wait::Prompt => ProviderError::InvalidFixture(format!("{} stalled", self.name)),
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel_rx.as_ref().is_some_and(|rx| *rx.borrow())
    }

    async fn handle(
        &mut self,
        message: Value,
        waiting_for: u64,
    ) -> Result<Incoming, ProviderError> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        let id = message.get("id").cloned();
        match (method, id) {
            (None, Some(id)) => {
                if id.as_u64() != Some(waiting_for) {
                    return Ok(Incoming::Other);
                }
                if let Some(error) = message.get("error") {
                    return Ok(Incoming::Response(Err(RpcError {
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("error without a message")
                            .to_string(),
                    })));
                }
                Ok(Incoming::Response(Ok(message
                    .get("result")
                    .cloned()
                    .unwrap_or(Value::Null))))
            }
            (Some(method), Some(id)) => {
                self.last_activity = Instant::now();
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                self.answer_request(&method, id, &params).await?;
                Ok(Incoming::Other)
            }
            (Some(method), None) => {
                if method == "session/update" {
                    self.last_activity = Instant::now();
                    if let Some(update) = message.pointer("/params/update") {
                        self.on_update(update);
                    }
                }
                Ok(Incoming::Other)
            }
            (None, None) => Ok(Incoming::Other),
        }
    }

    fn on_update(&mut self, update: &Value) {
        if self.replaying {
            return;
        }
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "agent_message_chunk" => {
                if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
                    self.agent_text.push_str(text);
                }
            }
            "tool_call" | "tool_call_update" => {
                if let Some(id) = update.get("toolCallId").and_then(Value::as_str) {
                    match update.get("status").and_then(Value::as_str) {
                        Some("completed") | Some("failed") => {
                            self.tools_in_flight.remove(id);
                        }
                        Some(_) => {
                            self.tools_in_flight.insert(id.to_string());
                        }
                        None if kind == "tool_call" => {
                            self.tools_in_flight.insert(id.to_string());
                        }
                        None => {}
                    }
                }
            }
            _ => {}
        }
        self.observer.on_update(update);
    }

    async fn answer_request(
        &mut self,
        method: &str,
        id: Value,
        params: &Value,
    ) -> Result<(), ProviderError> {
        let reply = if method == "session/request_permission" {
            let (allowed, title) = permission_decision(self.policy, params);
            if !allowed {
                self.refused.push(title.clone());
                tracing::info!(agent = %self.name, tool = %title, "refused a tool request in a read-only run");
            }
            json!({ "jsonrpc": "2.0", "id": id, "result": { "outcome": choose_permission_option(params, allowed) } })
        } else if self.policy == ToolPolicy::ReadOnly
            && (method == "fs/write_text_file" || method.starts_with("terminal/"))
        {
            let what = gated_request_title(method, params);
            self.refused.push(what.clone());
            tracing::info!(agent = %self.name, request = %what, "refused a write or command in a read-only run");
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": READ_ONLY_REFUSAL }
            })
        } else {
            // Coppice declares no other client capability, so these and
            // anything newer get a clear error instead of no answer.
            tracing::warn!(agent = %self.name, method, "unsupported ACP request from the agent");
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Coppice doesn't support {method}") }
            })
        };
        self.send(&reply).await
    }
}

/// Models from the `model` config option, or the older `models.availableModels`.
fn offered_models(session: &Value) -> Vec<(String, String)> {
    fn flatten(options: Option<&Value>, out: &mut Vec<(String, String)>) {
        for option in options.and_then(Value::as_array).into_iter().flatten() {
            if let Some(value) = option.get("value").and_then(Value::as_str) {
                let name = option.get("name").and_then(Value::as_str).unwrap_or(value);
                if !out.iter().any(|(v, _)| v == value) {
                    out.push((value.to_string(), name.to_string()));
                }
            }
            flatten(option.get("options"), out);
        }
    }
    let mut out = Vec::new();
    let model_option = session
        .get("configOptions")
        .and_then(Value::as_array)
        .and_then(|options| {
            options.iter().find(|o| {
                o.get("category").and_then(Value::as_str) == Some("model")
                    || o.get("id").and_then(Value::as_str) == Some("model")
            })
        });
    flatten(model_option.and_then(|o| o.get("options")), &mut out);
    if out.is_empty() {
        for model in session
            .pointer("/models/availableModels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = model.get("modelId").and_then(Value::as_str) {
                let name = model.get("name").and_then(Value::as_str).unwrap_or(id);
                if !out.iter().any(|(v, _)| v == id) {
                    out.push((id.to_string(), name.to_string()));
                }
            }
        }
    }
    out
}

/// An observer for a session look that streams nothing.
pub struct NoObserver;

impl AcpObserver for NoObserver {
    fn on_session(&mut self, _session_id: &str, _model: Option<&str>) {}
    fn on_update(&mut self, _update: &Value) {}
    fn on_end(&mut self) {}
}

/// JSON-RPC code ACP agents use for "sign in first".
const AUTH_REQUIRED: i64 = -32000;

const READ_ONLY_REFUSAL: &str =
    "Coppice: this run only allows reading. Writing files and running commands are refused.";

/// `write <path>` or `run <command>`, for the refused list.
fn gated_request_title(method: &str, params: &Value) -> String {
    if method == "fs/write_text_file" {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("a file");
        return format!("write {path}");
    }
    let command = params
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("a command");
    let args: Vec<&str> = params
        .get("args")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if args.is_empty() {
        format!("run {command}")
    } else {
        format!("run {command} {}", args.join(" "))
    }
}

async fn wait_cancel(rx: &mut Option<watch::Receiver<bool>>) {
    match rx {
        Some(rx) => loop {
            if *rx.borrow() {
                return;
            }
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        },
        None => std::future::pending::<()>().await,
    }
}

fn gateway_server(access: &McpAccess) -> Value {
    json!({
        "type": "http",
        "name": crate::mcp::protocol::SERVER_NAME,
        "url": access.url,
        "headers": [{ "name": "Authorization", "value": format!("Bearer {}", access.token) }]
    })
}

/// Flattens a select option list, which may hold groups of options.
fn option_values(options: Option<&Value>) -> Vec<String> {
    let mut values = Vec::new();
    for option in options.and_then(Value::as_array).into_iter().flatten() {
        if let Some(value) = option.get("value").and_then(Value::as_str) {
            values.push(value.to_string());
        }
        if let Some(group) = option.get("options") {
            values.extend(option_values(Some(group)));
        }
    }
    values
}

/// Tool kinds a read-only run may use. `think` and `fetch` change nothing on disk.
const READ_ONLY_KINDS: &[&str] = &["read", "search", "fetch", "think"];

fn permission_decision(policy: ToolPolicy, params: &Value) -> (bool, String) {
    let call = params.get("toolCall").cloned().unwrap_or(json!({}));
    let kind = call.get("kind").and_then(Value::as_str).unwrap_or("other");
    let title = call
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(kind)
        .to_string();
    let allowed = match policy {
        ToolPolicy::AllowAll => true,
        ToolPolicy::ReadOnly => READ_ONLY_KINDS.contains(&kind) || is_gateway_tool(&title),
    };
    (allowed, title)
}

/// The Coppice gateway's own tools, as the agents name them:
/// `mcp__coppice__x` (Kimi, Qwen) or `coppice-x` (Copilot).
fn is_gateway_tool(title: &str) -> bool {
    let server = crate::mcp::protocol::SERVER_NAME;
    title.starts_with(&format!("mcp__{server}__"))
        || title.starts_with(&format!("{server}-"))
        || title.starts_with(&format!("{server}_"))
}

fn choose_permission_option(params: &Value, allow: bool) -> Value {
    let preferred: &[&str] = if allow {
        &["allow_once", "allow_always"]
    } else {
        &["reject_once", "reject_always"]
    };
    let options = params.get("options").and_then(Value::as_array);
    let pick = preferred.iter().find_map(|kind| {
        options?
            .iter()
            .find(|option| option.get("kind").and_then(Value::as_str) == Some(kind))
    });
    match pick.and_then(|option| option.get("optionId")) {
        Some(option_id) => json!({ "outcome": "selected", "optionId": option_id }),
        // No matching option: cancelling is the only answer that never approves.
        None => json!({ "outcome": "cancelled" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission(kind: &str, options: Value) -> Value {
        json!({ "sessionId": "s", "toolCall": { "toolCallId": "t1", "kind": kind, "title": format!("{kind} thing") }, "options": options })
    }

    fn standard_options() -> Value {
        json!([
            { "optionId": "a1", "kind": "allow_once", "name": "Allow" },
            { "optionId": "a2", "kind": "allow_always", "name": "Always" },
            { "optionId": "r1", "kind": "reject_once", "name": "Reject" }
        ])
    }

    #[test]
    fn read_only_allows_reads_and_refuses_writes_and_shell() {
        for kind in ["read", "search", "fetch", "think"] {
            assert!(
                permission_decision(ToolPolicy::ReadOnly, &permission(kind, json!([]))).0,
                "{kind}"
            );
        }
        for kind in ["edit", "delete", "move", "execute", "other", "switch_mode"] {
            let (allowed, title) =
                permission_decision(ToolPolicy::ReadOnly, &permission(kind, json!([])));
            assert!(!allowed, "{kind}");
            assert_eq!(title, format!("{kind} thing"));
        }
        assert!(permission_decision(ToolPolicy::AllowAll, &permission("execute", json!([]))).0);
    }

    #[test]
    fn read_only_still_allows_the_gateway_tools() {
        for title in ["mcp__coppice__result_submit", "coppice-ticket_get", "coppice_result_submit"] {
            let params = json!({ "toolCall": { "kind": "other", "title": title }, "options": [] });
            assert!(permission_decision(ToolPolicy::ReadOnly, &params).0, "{title}");
        }
        for title in ["mcp__other__delete", "coppicex-run", "touch x"] {
            let params = json!({ "toolCall": { "kind": "other", "title": title }, "options": [] });
            assert!(!permission_decision(ToolPolicy::ReadOnly, &params).0, "{title}");
        }
    }

    #[test]
    fn a_missing_kind_counts_as_other() {
        let params = json!({ "toolCall": { "toolCallId": "t" }, "options": [] });
        let (allowed, title) = permission_decision(ToolPolicy::ReadOnly, &params);
        assert!(!allowed);
        assert_eq!(title, "other");
    }

    #[test]
    fn picks_the_once_options_and_falls_back() {
        let params = permission("edit", standard_options());
        assert_eq!(
            choose_permission_option(&params, true),
            json!({ "outcome": "selected", "optionId": "a1" })
        );
        assert_eq!(
            choose_permission_option(&params, false),
            json!({ "outcome": "selected", "optionId": "r1" })
        );

        let only_always = permission("edit", json!([{ "optionId": "x", "kind": "allow_always" }]));
        assert_eq!(
            choose_permission_option(&only_always, true),
            json!({ "outcome": "selected", "optionId": "x" })
        );
        // Never approve when asked to refuse and no refusal option exists.
        assert_eq!(
            choose_permission_option(&only_always, false),
            json!({ "outcome": "cancelled" })
        );
    }

    #[test]
    fn offered_models_read_the_config_option_or_the_older_list() {
        let config = json!({ "configOptions": [
            { "id": "mode", "category": "mode", "options": [{ "value": "yolo" }] },
            { "id": "model", "category": "model", "options": [
                { "value": "auto", "name": "Auto" },
                { "value": "auto", "name": "Auto again" },
                { "group": "g", "options": [{ "value": "gpt-5", "name": "GPT-5" }] }
            ] }
        ] });
        assert_eq!(
            offered_models(&config),
            [("auto".to_string(), "Auto".to_string()), ("gpt-5".to_string(), "GPT-5".to_string())]
        );
        let older = json!({ "models": { "availableModels": [
            { "modelId": "qwen3(openai)", "name": "Qwen 3" }, { "modelId": "x" }
        ] } });
        assert_eq!(
            offered_models(&older),
            [("qwen3(openai)".to_string(), "Qwen 3".to_string()), ("x".to_string(), "x".to_string())]
        );
        assert!(offered_models(&json!({})).is_empty());
    }

    #[test]
    fn model_values_flatten_groups() {
        let options = json!([
            { "value": "a/one", "name": "One" },
            { "group": "g", "name": "Group", "options": [{ "value": "b/two", "name": "Two" }] }
        ]);
        assert_eq!(option_values(Some(&options)), ["a/one", "b/two"]);
        assert!(option_values(None).is_empty());
    }

    #[test]
    fn gateway_header_carries_the_token() {
        let server = gateway_server(&McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "tok".into(),
        });
        assert_eq!(server["type"], "http");
        assert_eq!(server["name"], crate::mcp::protocol::SERVER_NAME);
        assert_eq!(server["headers"][0]["value"], "Bearer tok");
    }

    #[test]
    fn stop_reasons_read_as_sentences() {
        let outcome = |reason: &str, refused: Vec<String>| AcpOutcome {
            session_id: "s".into(),
            stop_reason: reason.into(),
            agent_text: String::new(),
            refused,
            stderr_tail: Vec::new(),
            models: Vec::new(),
        };
        assert!(outcome("end_turn", vec![])
            .stop_problem("OpenCode")
            .is_none());
        assert!(outcome("max_tokens", vec![])
            .stop_problem("OpenCode")
            .unwrap()
            .contains("token limit"));
        let refused = outcome("cancelled", vec!["touch x".into()])
            .stop_problem("OpenCode")
            .unwrap();
        assert!(
            refused.contains("touch x") && refused.contains("only allows tools that read"),
            "{refused}"
        );
    }
}
