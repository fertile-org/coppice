//! Local child-process MCP servers over newline-delimited JSON-RPC on stdio.

use super::client::{self, Redactor};
use super::transport::{McpConnection, McpTransport, ProxyError};
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use rmcp::transport::TokioChildProcess;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

/// Server-process variables a stdio child inherits; everything else is cleared.
const INHERITED_ENV: [&str; 4] = ["PATH", "HOME", "LANG", "TMPDIR"];
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

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args).env_clear();
        for name in INHERITED_ENV {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        cmd.envs(env).current_dir(cwd).kill_on_drop(true);

        let (process, stderr) = TokioChildProcess::builder(cmd)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProxyError::Start(redactor.apply(&format!("spawn failed: {e}"))))?;
        if let Some(stderr) = stderr {
            let redactor = redactor.clone();
            tokio::spawn(drain_stderr(BufReader::new(stderr), redactor, |line| {
                tracing::debug!(target: "coppice::plugin_mcp", %line, "mcp server stderr");
            }));
        }
        client::connect(process, redactor, Redactor::for_output(secrets)).await
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
