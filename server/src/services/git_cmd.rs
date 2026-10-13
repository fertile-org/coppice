//! One git entry point: absolute `git`, explicit PATH and HOME, no credential
//! prompt, and a deadline so a waiting push cannot hang the server.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use crate::host_env::{self, wait_blocking, GIT_TIMEOUT};

/// Run `git` in `cwd` (`None` keeps the server's working directory).
/// A non-zero exit is still `Ok`; the caller reads `status`.
pub async fn output<S: AsRef<OsStr>>(
    cwd: Option<&Path>,
    args: &[S],
) -> Result<std::process::Output, std::io::Error> {
    output_within(cwd, args, GIT_TIMEOUT).await
}

pub async fn output_within<S: AsRef<OsStr>>(
    cwd: Option<&Path>,
    args: &[S],
    timeout: Duration,
) -> Result<std::process::Output, std::io::Error> {
    let host = host_env::current();
    let mut cmd = host
        .tokio_command("git", args)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::NotFound, err.to_string()))?;
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.stdin(Stdio::null());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    match host_env::finish(cmd, timeout).await {
        host_env::RunOutput::NotFound(err) => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            err.to_string(),
        )),
        host_env::RunOutput::TimedOut { stderr_line } => {
            if let Some(line) = &stderr_line {
                tracing::debug!(%line, "git command timed out");
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("git timed out after {}s", timeout.as_secs()),
            ))
        }
        host_env::RunOutput::Exit(exit) => {
            let host_env::Exit {
                status,
                stdout,
                stderr,
                stderr_line,
            } = exit;
            let status = status.ok_or_else(|| {
                let err = stderr_line.unwrap_or_else(|| "unknown error".into());
                tracing::warn!(%err, "failed to start git");
                std::io::Error::other(format!("Couldn't start git: {err}"))
            })?;
            Ok(std::process::Output {
                status,
                stdout,
                stderr,
            })
        }
    }
}

/// Blocking `git` for callers that are not on the async runtime.
pub fn output_blocking<S: AsRef<OsStr>>(
    cwd: Option<&Path>,
    args: &[S],
) -> Result<std::process::Output, std::io::Error> {
    let host = host_env::current();
    let mut cmd = host
        .command("git", args)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::NotFound, err.to_string()))?;
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    match wait_blocking(cmd, GIT_TIMEOUT) {
        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
            tracing::debug!(error = %err, "git command timed out");
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("git timed out after {}s", GIT_TIMEOUT.as_secs()),
            ))
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to start git");
            Err(std::io::Error::other(format!("Couldn't start git: {err}")))
        }
        Ok(output) => Ok(output),
    }
}
