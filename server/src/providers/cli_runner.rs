//! The subprocess loop shared by the streaming-JSON CLI connectors: spawn,
//! record stdout and stderr, race cancel and deadline against stdout lines,
//! forward the first session id, and report the exit status.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::process_tree::{active, SpawnMeta};

const STDERR_TAIL_LINES: usize = 40;
const STDERR_DRAIN_AFTER_KILL: Duration = Duration::from_secs(1);

pub struct CliInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    /// Recorded on the process-group entry so task logs name the run.
    pub run_id: Option<String>,
    /// `<artifacts_dir>/runs/<run_id>/process-stops.log` receives stop lines.
    pub artifacts_dir: Option<String>,
    /// Set by the worker that already knows the ticket.
    pub ticket_id: Option<String>,
    /// When true, a terminal stdout event ends the run without waiting out a
    /// process that stays alive after the result. The flag comes from the
    /// connector's `run_contract`.
    pub result_before_exit: bool,
    /// How long to wait for exit after a result before stopping the group.
    /// `None` uses [`RESULT_EXIT_GRACE`].
    pub result_exit_grace: Option<Duration>,
    /// `(first output, idle)`; `None` uses [`FIRST_OUTPUT_TIMEOUT`] and [`IDLE_TIMEOUT`].
    pub stall_limits: Option<(Duration, Duration)>,
}

impl CliInvocation {
    fn first_output_timeout(&self) -> Duration {
        self.stall_limits
            .map_or(FIRST_OUTPUT_TIMEOUT, |(first, _)| first)
    }

    fn idle_timeout(&self) -> Duration {
        self.stall_limits.map_or(IDLE_TIMEOUT, |(_, idle)| idle)
    }
}

/// One log line per run on how the stream went, written when the run ends,
/// so a broken run shows which part of the protocol failed.
struct ProtocolSummary {
    program: String,
    run_id: Option<String>,
    started: std::time::Instant,
    lines: u64,
    json_lines: u64,
    terminal: bool,
    ended: &'static str,
    exit_code: Option<i32>,
}

impl ProtocolSummary {
    fn new(program: &str, run_id: Option<&str>) -> Self {
        Self {
            program: program.to_string(),
            run_id: run_id.map(str::to_string),
            started: std::time::Instant::now(),
            lines: 0,
            json_lines: 0,
            terminal: false,
            ended: "exited",
            exit_code: None,
        }
    }

    fn line(&mut self, json: bool) {
        self.lines += 1;
        if json {
            self.json_lines += 1;
        }
    }

    fn ended(&mut self, how: &'static str) {
        self.ended = how;
    }
}

impl Drop for ProtocolSummary {
    fn drop(&mut self) {
        tracing::info!(
            program = %self.program,
            run_id = self.run_id.as_deref().unwrap_or(""),
            lines = self.lines,
            json_lines = self.json_lines,
            other_lines = self.lines - self.json_lines,
            terminal_event = self.terminal,
            ended = self.ended,
            exit_code = self.exit_code,
            duration_ms = self.started.elapsed().as_millis() as u64,
            "cli stream summary"
        );
    }
}

/// A CLI that prints nothing this long after starting is stuck (sign-in
/// prompt, a flag it doesn't know, a hung startup).
pub const FIRST_OUTPUT_TIMEOUT: Duration = Duration::from_secs(3 * 60);
/// A CLI silent this long with no tool running has stalled. A running tool
/// (a long build or test suite) may be silent for longer.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// After a structured result, wait this long for the CLI to exit, then stop
/// the process group. Claude Code 2.1.292+ keeps running while background
/// commands finish; the same grace covers every streaming CLI that sets
/// `result_before_exit`. The run timeout still applies when no result arrives.
pub const RESULT_EXIT_GRACE: Duration = Duration::from_secs(2);

pub trait LineHandler: Send {
    /// One parsed JSON stdout line. Non-JSON lines are logged and skipped.
    /// The handler forwards console events, accumulates text, and says whether
    /// the stream reached its terminal event.
    fn on_json(&mut self, value: &Value) -> LineStep;

    /// Whether a tool the agent started has not reported back yet. Silence
    /// then is the tool working, not a stall.
    fn tools_running(&self) -> bool {
        false
    }

    /// Lines to write to the CLI's stdin, for runs started with
    /// [`run_cli_interactive`] (answers to control requests). Ignored otherwise.
    fn take_replies(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Tool calls started and not yet finished, by call id.
#[derive(Debug, Default)]
pub struct ToolTracker {
    running: std::collections::HashSet<String>,
}

impl ToolTracker {
    pub fn started(&mut self, id: &str) {
        self.running.insert(id.to_string());
    }

    pub fn finished(&mut self, id: &str) {
        self.running.remove(id);
    }

    pub fn any(&self) -> bool {
        !self.running.is_empty()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LineStep {
    pub session_id: Option<String>,
    pub stop: bool,
}

pub struct RunIo {
    pub cancel_rx: Option<watch::Receiver<bool>>,
    pub session_created_tx: Option<watch::Sender<String>>,
}

#[derive(Debug)]
pub struct CliExit {
    pub status: ExitStatus,
    /// First 40 stderr lines.
    pub stderr_tail: Vec<String>,
    /// The runner stopped on a terminal result under [`CliInvocation::result_before_exit`].
    /// A non-zero status in that case is not a failed run.
    pub had_terminal_result: bool,
    /// The process was still alive after the grace period and was stopped.
    pub stopped_after_result: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Couldn't start the CLI: {0}")]
    Spawn(std::io::Error),
    #[error("CLI run cancelled")]
    Cancelled,
    #[error("CLI run timed out")]
    TimedOut { stderr_tail: Vec<String> },
    /// Stopped by [`FIRST_OUTPUT_TIMEOUT`] or [`IDLE_TIMEOUT`]; `reason` is a sentence.
    #[error("{reason}")]
    Stalled {
        reason: String,
        stderr_tail: Vec<String>,
    },
    #[error("CLI I/O error: {0}")]
    Io(std::io::Error),
}

pub(super) fn resolve_cli_program(
    host: &crate::host_env::HostEnv,
    program: &str,
) -> Result<String, CliError> {
    if program.contains('/') || program.contains('\\') {
        return Ok(program.to_string());
    }
    host.resolve(program)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|err| {
            CliError::Spawn(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                err.to_string(),
            ))
        })
}

pub async fn run_cli(
    inv: CliInvocation,
    handler: &mut dyn LineHandler,
    io: RunIo,
) -> Result<CliExit, CliError> {
    run_cli_with_stdin(inv, None, handler, io).await
}

/// Like [`run_cli`], but writes `stdin` to the child and closes it. With
/// `None`, the child's stdin is null.
pub async fn run_cli_with_stdin(
    inv: CliInvocation,
    stdin: Option<String>,
    handler: &mut dyn LineHandler,
    io: RunIo,
) -> Result<CliExit, CliError> {
    let mode = match stdin {
        Some(input) => StdinMode::Once(input),
        None => StdinMode::Null,
    };
    run_cli_inner(inv, mode, handler, io).await
}

/// Keeps stdin open: writes `initial` lines, then the handler's replies, and
/// closes stdin once the terminal event is read. Writing happens on its own
/// task so a CLI that stops reading can't stall reading its output.
pub async fn run_cli_interactive(
    inv: CliInvocation,
    initial: Vec<String>,
    handler: &mut dyn LineHandler,
    io: RunIo,
) -> Result<CliExit, CliError> {
    run_cli_inner(inv, StdinMode::Interactive(initial), handler, io).await
}

enum StdinMode {
    Null,
    Once(String),
    Interactive(Vec<String>),
}

async fn run_cli_inner(
    inv: CliInvocation,
    stdin: StdinMode,
    handler: &mut dyn LineHandler,
    io: RunIo,
) -> Result<CliExit, CliError> {
    let host = crate::host_env::current();
    let program = resolve_cli_program(&host, &inv.program)?;
    let secrets =
        crate::logs::guard_for_child_env(inv.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let mut cmd = Command::new(&program);
    cmd.env("PATH", &host.path);
    if !host.home.as_os_str().is_empty() {
        cmd.env("HOME", &host.home);
    }
    super::env_hygiene::scrub(&mut cmd);
    cmd.args(&inv.args)
        .envs(inv.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(&inv.cwd)
        .stdin(if matches!(stdin, StdinMode::Null) {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut tracked = active()
        .spawn(
            &mut cmd,
            SpawnMeta {
                command: inv.program.clone(),
                label: "cli",
                run_id: inv.run_id.clone(),
                task_log_dir: task_log_dir(&inv),
            },
        )
        .map_err(CliError::Spawn)?;

    let mut replies: Option<tokio::sync::mpsc::UnboundedSender<String>> = None;
    let stdin_task = match stdin {
        StdinMode::Null => None,
        StdinMode::Once(input) => {
            let mut pipe = tracked.take_stdin().expect("piped stdin");
            Some(tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = pipe.write_all(input.as_bytes()).await;
                let _ = pipe.shutdown().await;
            }))
        }
        StdinMode::Interactive(initial) => {
            let mut pipe = tracked.take_stdin().expect("piped stdin");
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            for line in initial {
                let _ = tx.send(line);
            }
            replies = Some(tx);
            Some(tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                while let Some(mut line) = rx.recv().await {
                    if !line.ends_with('\n') {
                        line.push('\n');
                    }
                    if pipe.write_all(line.as_bytes()).await.is_err() || pipe.flush().await.is_err()
                    {
                        break;
                    }
                }
                let _ = pipe.shutdown().await;
            }))
        }
    };
    let stdout = tracked.take_stdout().expect("piped stdout");
    let stderr = tracked.take_stderr().expect("piped stderr");
    let stderr_tail = Arc::new(Mutex::new(Vec::new()));
    let mut stderr_task = tokio::spawn(pump_stderr(
        stderr,
        secrets.clone(),
        inv.run_id.clone(),
        inv.ticket_id.clone(),
        stderr_tail.clone(),
    ));
    // `secrets` stays alive for stdout lines read on this task. The stderr task
    // holds a clone until that pipe is drained.

    let mut reader = super::line_reader::CappedLines::new(
        BufReader::new(stdout),
        super::line_reader::MAX_LINE_BYTES,
    );
    let started = tokio::time::Instant::now();
    let deadline = started + inv.timeout;
    let mut cancel_rx = io.cancel_rx;
    let mut session_sent = false;
    let mut stopped_on_result = false;
    let mut summary = ProtocolSummary::new(&inv.program, inv.run_id.as_deref());
    let mut last_output: Option<tokio::time::Instant> = None;

    loop {
        let now = tokio::time::Instant::now();
        let stall_at = match last_output {
            None => started + inv.first_output_timeout(),
            Some(_) if handler.tools_running() => deadline,
            Some(at) => at + inv.idle_timeout(),
        };
        if now >= stall_at && stall_at < deadline {
            let reason = match last_output {
                None => format!(
                    "the CLI printed nothing in its first {} seconds, so Coppice stopped it",
                    inv.first_output_timeout().as_secs()
                ),
                Some(_) => format!(
                    "the CLI printed nothing for {} minutes with no tool running, so Coppice stopped it",
                    inv.idle_timeout().as_secs() / 60
                ),
            };
            summary.ended("stalled");
            let _ = tracked.shutdown("stalled").await;
            if tokio::time::timeout(STDERR_DRAIN_AFTER_KILL, &mut stderr_task)
                .await
                .is_err()
            {
                stderr_task.abort();
            }
            let stderr_tail = std::mem::take(&mut *stderr_tail.lock().unwrap());
            return Err(CliError::Stalled {
                reason,
                stderr_tail,
            });
        }

        if is_cancelled(&cancel_rx) {
            summary.ended("cancelled");
            let _ = tracked.shutdown("cancelled").await;
            return Err(CliError::Cancelled);
        }

        tokio::select! {
            biased;

            _ = wait_cancel(&mut cancel_rx) => {
                if is_cancelled(&cancel_rx) {
                    summary.ended("cancelled");
                    let _ = tracked.shutdown("cancelled").await;
                    return Err(CliError::Cancelled);
                }
            }

            _ = tokio::time::sleep_until(stall_at.min(deadline)), if stall_at < deadline => {
                continue;
            }

            _ = tokio::time::sleep_until(deadline) => {
                summary.ended("timed out");
                let _ = tracked.shutdown("timed out").await;
                // A grandchild can hold the inherited stderr pipe open past the kill.
                if tokio::time::timeout(STDERR_DRAIN_AFTER_KILL, &mut stderr_task)
                    .await
                    .is_err()
                {
                    stderr_task.abort();
                }
                let stderr_tail = std::mem::take(&mut *stderr_tail.lock().unwrap());
                return Err(CliError::TimedOut { stderr_tail });
            }

            line = reader.next_line() => {
                match line {
                    Ok(Some(raw)) => {
                        last_output = Some(tokio::time::Instant::now());
                        let parsed = serde_json::from_str::<Value>(&raw);
                        summary.line(parsed.is_ok());
                        // A run's JSON lines become its console, which is kept
                        // in the run folder. Only other stdout goes to the
                        // server log.
                        if parsed.is_err() || inv.run_id.is_none() {
                            crate::logs::log_cli_line(
                                inv.run_id.as_deref(),
                                inv.ticket_id.as_deref(),
                                "stdout",
                                &raw,
                            );
                        }
                        let Ok(value) = parsed else {
                            continue;
                        };
                        let step = handler.on_json(&value);
                        if let Some(tx) = &replies {
                            for reply in handler.take_replies() {
                                let _ = tx.send(reply);
                            }
                        }
                        if !session_sent {
                            if let Some(sid) = step.session_id.filter(|s| !s.is_empty()) {
                                if let Some(tx) = &io.session_created_tx {
                                    let _ = tx.send(sid);
                                }
                                session_sent = true;
                            }
                        }
                        if step.stop {
                            stopped_on_result = true;
                            summary.terminal = true;
                            // Closing stdin tells an interactive CLI the turn is over.
                            replies = None;
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        summary.ended("read error");
                        let _ = tracked.shutdown("io error").await;
                        return Err(CliError::Io(e));
                    }
                }
            }
        }
    }

    drop(replies);
    let finished = if stopped_on_result && inv.result_before_exit {
        summary.ended("result");
        finish_after_result(
            &mut tracked,
            &mut cancel_rx,
            stdin_task,
            &mut stderr_task,
            &stderr_tail,
            inv.result_exit_grace.unwrap_or(RESULT_EXIT_GRACE),
        )
        .await
    } else {
        let status = tracked
            .wait_and_reap("run ended")
            .await
            .map_err(CliError::Io)?;
        finish_ok(
            stdin_task,
            &mut stderr_task,
            &stderr_tail,
            status,
            false,
            false,
        )
        .await
    };
    match &finished {
        Ok(exit) => summary.exit_code = exit.status.code(),
        Err(CliError::Cancelled) => summary.ended("cancelled"),
        Err(_) => {}
    }
    finished
}

enum AfterResult {
    Cancelled,
    Exited(ExitStatus),
    Grace,
}

/// The structured result is in. Give the process a short chance to exit, then
/// stop the group. Cancel during the grace is still a cancel.
async fn finish_after_result(
    tracked: &mut crate::process_tree::TrackedProcess,
    cancel_rx: &mut Option<watch::Receiver<bool>>,
    stdin_task: Option<tokio::task::JoinHandle<()>>,
    stderr_task: &mut tokio::task::JoinHandle<()>,
    stderr_tail: &Arc<Mutex<Vec<String>>>,
    grace: Duration,
) -> Result<CliExit, CliError> {
    let outcome = tokio::select! {
        biased;
        _ = wait_cancel(cancel_rx) => AfterResult::Cancelled,
        status = tracked.wait_and_reap("run ended") => {
            AfterResult::Exited(status.map_err(CliError::Io)?)
        }
        _ = tokio::time::sleep(grace) => AfterResult::Grace,
    };
    match outcome {
        AfterResult::Cancelled => {
            let _ = tracked.shutdown("cancelled").await;
            Err(CliError::Cancelled)
        }
        AfterResult::Exited(status) => {
            finish_ok(stdin_task, stderr_task, stderr_tail, status, true, false).await
        }
        AfterResult::Grace => {
            let status = tracked
                .shutdown("result delivered; process still running")
                .await
                .map_err(CliError::Io)?;
            if tokio::time::timeout(STDERR_DRAIN_AFTER_KILL, &mut *stderr_task)
                .await
                .is_err()
            {
                stderr_task.abort();
            }
            let tail = std::mem::take(&mut *stderr_tail.lock().unwrap());
            if let Some(task) = stdin_task {
                let _ = task.await;
            }
            Ok(CliExit {
                status,
                stderr_tail: tail,
                had_terminal_result: true,
                stopped_after_result: true,
            })
        }
    }
}

async fn finish_ok(
    stdin_task: Option<tokio::task::JoinHandle<()>>,
    stderr_task: &mut tokio::task::JoinHandle<()>,
    stderr_tail: &Arc<Mutex<Vec<String>>>,
    status: ExitStatus,
    had_terminal_result: bool,
    stopped_after_result: bool,
) -> Result<CliExit, CliError> {
    if let Some(task) = stdin_task {
        let _ = task.await;
    }
    let _ = stderr_task.await;
    let stderr_tail = std::mem::take(&mut *stderr_tail.lock().unwrap());
    Ok(CliExit {
        status,
        stderr_tail,
        had_terminal_result,
        stopped_after_result,
    })
}

/// One run-log line: binary name, version token, and the argv that was launched.
/// Env values are not included.
pub fn log_launch(
    program: &str,
    version: &str,
    args: &[String],
    log_dir: Option<&std::path::Path>,
) {
    let name = std::path::Path::new(program)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(program);
    let line = format!("{name} {version} · flags: {}", args.join(" "));
    tracing::info!(target: "cli_runner", "{line}");
    let Some(dir) = log_dir else {
        return;
    };
    if let Err(err) = (|| {
        std::fs::create_dir_all(dir)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("run.log"))?;
        use std::io::Write;
        writeln!(file, "{line}")
    })() {
        tracing::debug!(%err, "failed to write run.log");
    }
}

/// Records stderr and keeps the first lines for error messages.
pub(super) async fn pump_stderr(
    stderr: impl AsyncRead + Unpin,
    _secrets: crate::logs::SecretGuard,
    run_id: Option<String>,
    ticket_id: Option<String>,
    tail: Arc<Mutex<Vec<String>>>,
) {
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        crate::logs::log_cli_line(run_id.as_deref(), ticket_id.as_deref(), "stderr", &line);
        let mut lines = tail.lock().unwrap();
        if lines.len() < STDERR_TAIL_LINES {
            lines.push(line);
        }
    }
}

fn task_log_dir(inv: &CliInvocation) -> Option<PathBuf> {
    run_log_dir(inv.artifacts_dir.as_deref(), inv.run_id.as_deref())
}

pub fn run_log_dir(artifacts_dir: Option<&str>, run_id: Option<&str>) -> Option<PathBuf> {
    let (Some(dir), Some(run_id)) = (artifacts_dir, run_id) else {
        return None;
    };
    if dir.is_empty() || run_id.is_empty() {
        return None;
    }
    Some(PathBuf::from(dir).join("runs").join(run_id))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_launch_writes_version_and_flags_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_dir = dir.path().join("runs").join("r1");
        log_launch(
            "/usr/bin/claude",
            "2.1.292",
            &["-p".into(), "--verbose".into()],
            Some(&log_dir),
        );
        let text = std::fs::read_to_string(log_dir.join("run.log")).expect("run.log");
        assert_eq!(text, "claude 2.1.292 · flags: -p --verbose\n");
        assert!(!text.contains("ANTHROPIC"));
    }

    #[test]
    fn stdout_and_stderr_lines_are_stored_with_the_run_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::logs::with_cli_logs(dir.path(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(async {
                let mut handler = NopHandler;
                let invocation = CliInvocation {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        "printf '%s\\n' 'plain-line'; printf '%s\\n' '{\"ok\":true}'; printf '%s\\n' 'err-line' >&2"
                            .into(),
                    ],
                    env: Vec::new(),
                    cwd: std::env::temp_dir(),
                    timeout: Duration::from_secs(10),
                    run_id: Some("run-42".into()),
                    artifacts_dir: None,
                    ticket_id: Some("ticket-9".into()),
                    result_before_exit: false,
                    result_exit_grace: None,
                    stall_limits: None,
                };
                run_cli(
                    invocation,
                    &mut handler,
                    RunIo {
                        cancel_rx: None,
                        session_created_tx: None,
                    },
                )
                .await
                .expect("sh");
            });
        })
        .expect("subscriber");

        let lines = jsonl_values(dir.path());
        let plain = find_line(&lines, "plain-line");
        assert_eq!(plain["fields"]["source"], "cli");
        assert_eq!(plain["fields"]["stream"], "stdout");
        assert_eq!(plain["fields"]["run_id"], "run-42");
        assert_eq!(plain["fields"]["ticket_id"], "ticket-9");
        assert_eq!(plain["level"], "INFO");
        let timestamp = plain["timestamp"].as_str().expect("timestamp");
        assert!(
            timestamp.contains('T') && timestamp.ends_with('Z'),
            "{timestamp}"
        );

        // A run's JSON lines are its console, kept in the run folder.
        assert!(
            !lines
                .iter()
                .any(|line| line["fields"]["message"] == "{\"ok\":true}"),
            "JSON stdout of a run was mirrored: {lines:?}"
        );

        let err = find_line(&lines, "err-line");
        assert_eq!(err["fields"]["stream"], "stderr");
        assert_eq!(err["fields"]["run_id"], "run-42");
        assert_eq!(err["fields"]["ticket_id"], "ticket-9");
        assert_eq!(
            lines
                .iter()
                .filter(|line| line["fields"]["message"] == "err-line")
                .count(),
            1
        );
    }

    struct NopHandler;

    impl LineHandler for NopHandler {
        fn on_json(&mut self, _value: &Value) -> LineStep {
            LineStep::default()
        }
    }

    fn jsonl_values(dir: &std::path::Path) -> Vec<Value> {
        let mut lines = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !(name.starts_with("coppice.") && name.ends_with(".jsonl")) {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("jsonl");
            for line in text.lines() {
                lines.push(serde_json::from_str(line).expect("json"));
            }
        }
        lines
    }

    fn find_line<'a>(lines: &'a [Value], message: &str) -> &'a Value {
        lines
            .iter()
            .find(|line| line["fields"]["message"] == message)
            .unwrap_or_else(|| panic!("missing {message} in {lines:?}"))
    }
}
