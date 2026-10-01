//! The subprocess loop shared by the streaming-JSON CLI connectors: spawn,
//! mirror stderr to tracing, race cancel and deadline against stdout lines,
//! forward the first session id, and report the exit status.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

const STDERR_TAIL_LINES: usize = 40;

pub struct CliInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub timeout: Duration,
}

pub trait LineHandler: Send {
    /// One parsed JSON stdout line (non-JSON lines are skipped by the runner).
    /// The handler forwards console events, accumulates text, and says whether
    /// the stream reached its terminal event.
    fn on_json(&mut self, value: &Value) -> LineStep;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LineStep {
    pub session_id: Option<String>,
    pub stop: bool,
}

pub struct RunIo {
    pub cancel_rx: Option<watch::Receiver<bool>>,
    pub session_created_tx: Option<watch::Sender<String>>,
    /// Tracing target for mirrored stderr lines, e.g. `"cursor.stderr"`.
    pub stderr_target: &'static str,
}

#[derive(Debug)]
pub struct CliExit {
    pub status: ExitStatus,
    /// First 40 stderr lines.
    pub stderr_tail: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("failed to spawn CLI: {0}")]
    Spawn(std::io::Error),
    #[error("CLI run cancelled")]
    Cancelled,
    #[error("CLI run timed out")]
    TimedOut { stderr_tail: Vec<String> },
    #[error("CLI I/O error: {0}")]
    Io(std::io::Error),
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
    let mut cmd = Command::new(&inv.program);
    cmd.args(&inv.args)
        .envs(inv.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(&inv.cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(CliError::Spawn)?;

    let stdin_task = stdin.map(|input| {
        let mut pipe = child.stdin.take().expect("piped stdin");
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let _ = pipe.write_all(input.as_bytes()).await;
            let _ = pipe.shutdown().await;
        })
    });
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_task = tokio::spawn(pump_stderr(stderr, io.stderr_target));

    let mut reader = BufReader::new(stdout).lines();
    let deadline = tokio::time::Instant::now() + inv.timeout;
    let mut cancel_rx = io.cancel_rx;
    let mut session_sent = false;

    loop {
        if is_cancelled(&cancel_rx) {
            let _ = child.kill().await;
            return Err(CliError::Cancelled);
        }

        tokio::select! {
            biased;

            _ = wait_cancel(&mut cancel_rx) => {
                if is_cancelled(&cancel_rx) {
                    let _ = child.kill().await;
                    return Err(CliError::Cancelled);
                }
            }

            _ = tokio::time::sleep_until(deadline) => {
                let _ = child.kill().await;
                let stderr_tail = stderr_task.await.unwrap_or_default();
                return Err(CliError::TimedOut { stderr_tail });
            }

            line = reader.next_line() => {
                match line {
                    Ok(Some(raw)) => {
                        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                            continue;
                        };
                        let step = handler.on_json(&value);
                        if !session_sent {
                            if let Some(sid) = step.session_id.filter(|s| !s.is_empty()) {
                                if let Some(tx) = &io.session_created_tx {
                                    let _ = tx.send(sid);
                                }
                                session_sent = true;
                            }
                        }
                        if step.stop {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(CliError::Io(e));
                    }
                }
            }
        }
    }

    let status = child.wait().await.map_err(CliError::Io)?;
    if let Some(task) = stdin_task {
        let _ = task.await;
    }
    let stderr_tail = stderr_task.await.unwrap_or_default();
    Ok(CliExit {
        status,
        stderr_tail,
    })
}

/// Mirrors stderr to tracing and keeps the first lines for error messages.
/// Tracing targets must be compile-time constants, hence the dispatch.
async fn pump_stderr(stderr: impl AsyncRead + Unpin, target: &'static str) -> Vec<String> {
    let mut reader = BufReader::new(stderr).lines();
    let mut lines = Vec::new();
    while let Ok(Some(line)) = reader.next_line().await {
        match target {
            "claude_code.stderr" => tracing::debug!(target: "claude_code.stderr", "{line}"),
            "codex.stderr" => tracing::debug!(target: "codex.stderr", "{line}"),
            "cursor.stderr" => tracing::debug!(target: "cursor.stderr", "{line}"),
            "kilo_code.stderr" => tracing::debug!(target: "kilo_code.stderr", "{line}"),
            _ => tracing::debug!(target: "cli_runner.stderr", source = target, "{line}"),
        }
        if lines.len() < STDERR_TAIL_LINES {
            lines.push(line);
        }
    }
    lines
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
