//! Local child-process MCP servers over newline-delimited JSON-RPC on stdio.

use super::client::{self, Redactor};
use super::transport::{McpConnection, McpTransport, ProxyError};
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use rmcp::transport::TokioChildProcess;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Server-process variables a stdio child inherits; everything else is cleared.
const INHERITED_ENV: [&str; 4] = ["PATH", "HOME", "LANG", "TMPDIR"];

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
        let ResolvedTransport::Stdio { command, args, env } = spec else {
            return Err(ProxyError::Start(format!(
                "stdio transport cannot open a {} server",
                spec.kind()
            )));
        };
        let redactor = std::iter::once(command)
            .chain(args)
            .chain(env.values())
            .fold(Redactor::default(), |r, value| r.secret(value));

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
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "coppice::plugin_mcp", %line, "mcp server stderr");
                }
            });
        }
        client::connect(process, redactor).await
    }
}
