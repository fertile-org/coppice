//! Local child-process MCP servers over newline-delimited JSON-RPC on stdio.

use super::client::{self, Redactor};
use super::transport::{McpConnection, McpTransport, ProxyError};
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use rmcp::transport::TokioChildProcess;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

/// Bytes kept per stderr line; the rest of an overlong line is dropped.
const MAX_STDERR_LINE: usize = 8 * 1024;

/// Reads stderr until EOF or an I/O error so the child never sees a closed pipe.
async fn drain_stderr<R, F>(mut reader: R, redactor: Redactor, mut log: F)
where
    R: AsyncBufRead + Unpin,
    F: FnMut(String),
{
    let mut line = Vec::with_capacity(256);
    let mut truncated = false;
    loop {
        let chunk = match reader.fill_buf().await {
            Ok(chunk) => chunk,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if chunk.is_empty() {
            if !line.is_empty() {
                log(drain_line(&line, truncated, &redactor));
            }
            break;
        }
        let newline = chunk.iter().position(|&b| b == b'\n');
        let part = &chunk[..newline.unwrap_or(chunk.len())];
        let room = MAX_STDERR_LINE - line.len();
        line.extend_from_slice(&part[..part.len().min(room)]);
        truncated |= part.len() > room;
        let used = newline.map_or(chunk.len(), |i| i + 1);
        reader.consume(used);
        if newline.is_some() {
            log(drain_line(&line, truncated, &redactor));
            line.clear();
            truncated = false;
        }
    }
}

fn drain_line(buf: &[u8], truncated: bool, redactor: &Redactor) -> String {
    let buf = buf.strip_suffix(b"\r").unwrap_or(buf);
    let mut line = redactor.apply(&String::from_utf8_lossy(buf));
    if truncated {
        line.push_str(" [truncated]");
    }
    line
}

pub struct StdioTransport;

#[async_trait]
impl McpTransport for StdioTransport {
    fn kind(&self) -> &'static str {
        "stdio"
    }

    async fn connect(
        &self,
        spec: &ResolvedTransport,
        cwd: &Path,
    ) -> Result<Box<dyn McpConnection>, ProxyError> {
        let ResolvedTransport::Stdio {
            command,
            args,
            env,
            secrets,
        } = spec
        else {
            return Err(ProxyError::Start(format!(
                "stdio transport cannot open a {} server",
                spec.kind()
            )));
        };
        let redactor = std::iter::once(command)
            .chain(args)
            .chain(env.values())
            .fold(Redactor::for_errors(secrets), |r, value| r.secret(value));

        let host = crate::host_env::current();
        let program = host.resolve(command).map_err(|err| {
            ProxyError::Start(
                redactor.apply(&format!("MCP server `{command}` wasn't found: {err}")),
            )
        })?;
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(args).env_clear().env("PATH", &host.path);
        if !host.home.as_os_str().is_empty() {
            cmd.env("HOME", &host.home);
        }
        for name in ["LANG", "TMPDIR"] {
            if let Some(value) = host.var(name) {
                cmd.env(name, value);
            }
        }
        cmd.envs(env).current_dir(cwd).kill_on_drop(true);

        let (process, stderr) = TokioChildProcess::builder(cmd)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProxyError::Start(redactor.apply(&format!("spawn failed: {e}"))))?;
        let stderr_line = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        if let Some(stderr) = stderr {
            let redactor = redactor.clone();
            let stderr_line = std::sync::Arc::clone(&stderr_line);
            tokio::spawn(drain_stderr(
                BufReader::new(stderr),
                redactor,
                move |line| {
                    tracing::debug!(target: "coppice::plugin_mcp", %line, "mcp server stderr");
                    if let Ok(mut slot) = stderr_line.lock() {
                        if slot.is_none() && !line.trim().is_empty() {
                            *slot = Some(line);
                        }
                    }
                },
            ));
        }
        let started = client::connect(process, redactor.clone(), Redactor::for_output(secrets));
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, started).await {
            Ok(result) => result.map_err(|err| annotate_start(err, &stderr_line, &redactor)),
            Err(_) => Err(ProxyError::Start(redactor.apply(&format!(
                "MCP server timed out after {}s{}",
                HANDSHAKE_TIMEOUT.as_secs(),
                stderr_suffix(&stderr_line)
            )))),
        }
    }
}

const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

fn stderr_suffix(slot: &std::sync::Mutex<Option<String>>) -> String {
    slot.lock()
        .ok()
        .and_then(|guard| guard.clone())
        .filter(|line| !line.is_empty())
        .and_then(|line| crate::host_env::first_text_line(&line))
        .map(|line| format!(": {line}"))
        .unwrap_or_default()
}

fn annotate_start(
    err: ProxyError,
    slot: &std::sync::Mutex<Option<String>>,
    redactor: &Redactor,
) -> ProxyError {
    let suffix = stderr_suffix(slot);
    if suffix.is_empty() {
        return err;
    }
    match err {
        ProxyError::Start(message) => {
            ProxyError::Start(redactor.apply(&format!("{message}{suffix}")))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn drained(input: &[u8], redactor: Redactor) -> Vec<String> {
        let mut lines = Vec::new();
        drain_stderr(input, redactor, |line| lines.push(line)).await;
        lines
    }

    #[test]
    fn drain_line_is_lossy_and_redacted() {
        let redactor = Redactor::default().secret("s3cr3t");
        assert_eq!(
            drain_line(b"token s3cr3t \xff\xfe ok\r", false, &redactor),
            "token [redacted] \u{FFFD}\u{FFFD} ok"
        );
        assert_eq!(drain_line(b"abc", true, &redactor), "abc [truncated]");
    }

    #[tokio::test]
    async fn drain_survives_invalid_utf8_and_reads_to_eof() {
        let redactor = Redactor::default().secret("s3cr3t");
        let lines = drained(b"\xff\xfe\nkey=s3cr3t\nlast", redactor).await;
        assert_eq!(lines, ["\u{FFFD}\u{FFFD}", "key=[redacted]", "last"]);
    }

    #[tokio::test]
    async fn drain_caps_overlong_lines() {
        let mut input = vec![b'a'; MAX_STDERR_LINE * 3];
        input.extend_from_slice(b"\nnext\n");
        let lines = drained(&input, Redactor::default()).await;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), MAX_STDERR_LINE + " [truncated]".len());
        assert!(lines[0].ends_with(" [truncated]"));
        assert_eq!(lines[1], "next");
    }
}
