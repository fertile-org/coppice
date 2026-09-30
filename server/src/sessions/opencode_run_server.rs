//! One `opencode serve` process per run, so each run can carry its own MCP
//! configuration and bearer token.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::process::Child;

use crate::mcp::grant::McpAccess;

const START_ATTEMPTS: u32 = 3;
const HEALTH_DEADLINE: Duration = Duration::from_secs(30);

/// The per-run `OPENCODE_CONFIG`. The token stays in the process environment;
/// OpenCode interpolates `{env:COPPICE_MCP_TOKEN}` into the header.
pub fn opencode_run_config(access: Option<&McpAccess>) -> serde_json::Value {
    let mut config = serde_json::json!({ "$schema": "https://opencode.ai/config.json" });
    if let Some(access) = access {
        config["mcp"] = serde_json::json!({
            "coppice": {
                "type": "remote",
                "url": access.url,
                "enabled": true,
                "headers": { "Authorization": "Bearer {env:COPPICE_MCP_TOKEN}" },
            }
        });
    }
    config
}

struct RunServer {
    base_url: String,
    child: Child,
}

pub struct OpenCodeRunServers {
    command: String,
    hostname: String,
    runs: Mutex<HashMap<String, RunServer>>,
}

impl OpenCodeRunServers {
    pub fn new(command: String, hostname: String) -> Arc<Self> {
        Arc::new(Self {
            command,
            hostname,
            runs: Mutex::new(HashMap::new()),
        })
    }

    /// Spawns `opencode serve` on a free port with `OPENCODE_CONFIG=config_path`
    /// and waits until it answers `/doc`.
    pub async fn start(
        self: &Arc<Self>,
        key: &str,
        config_path: &Path,
        env: Vec<(&'static str, String)>,
    ) -> anyhow::Result<OpenCodeRunLease> {
        let mut attempt = 1;
        loop {
            let port = free_port(&self.hostname)?;
            let base_url = format!("http://{}:{port}", self.hostname);
            let mut child = self.spawn(port, config_path, &env)?;

            match wait_for_healthy(&base_url, &mut child).await {
                Ok(()) => return self.register(key, base_url, child).await,
                // Another process can grab the port between probing and binding.
                Err(Unhealthy::Exited(_)) if attempt < START_ATTEMPTS => {
                    tracing::warn!(%base_url, attempt, "opencode serve exited during startup; retrying");
                    attempt += 1;
                }
                Err(err) => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return Err(err.into_error(&base_url));
                }
            }
        }
    }

    pub fn base_url(&self, key: &str) -> Option<String> {
        self.lock().get(key).map(|run| run.base_url.clone())
    }

    pub async fn shutdown_all(&self) {
        let runs: Vec<RunServer> = self.lock().drain().map(|(_, run)| run).collect();
        for mut run in runs {
            let _ = run.child.start_kill();
            let _ = run.child.wait().await;
        }
    }

    fn spawn(
        &self,
        port: u16,
        config_path: &Path,
        env: &[(&'static str, String)],
    ) -> anyhow::Result<Child> {
        tokio::process::Command::new(&self.command)
            .args([
                "serve",
                "--hostname",
                &self.hostname,
                "--port",
                &port.to_string(),
            ])
            .env("OPENCODE_CONFIG", config_path)
            .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    anyhow::anyhow!(
                        "opencode binary `{}` not found on PATH. Install it where the server runs, \
                         or disable OpenCode in config.toml \
                         (agent.connectors.opencode.enabled = false and \
                         agent.default_connector = \"mock\" or another connector)",
                        self.command
                    )
                } else {
                    anyhow::anyhow!("failed to spawn `{} serve`: {err}", self.command)
                }
            })
    }

    async fn register(
        self: &Arc<Self>,
        key: &str,
        base_url: String,
        mut child: Child,
    ) -> anyhow::Result<OpenCodeRunLease> {
        let conflict = {
            let mut runs = self.lock();
            if runs.contains_key(key) {
                format!("an opencode serve for run {key} is already running")
            } else if let Some((owner, _)) = runs.iter().find(|(_, run)| run.base_url == base_url) {
                format!("{base_url} is already serving run {owner}")
            } else {
                runs.insert(
                    key.to_string(),
                    RunServer {
                        base_url: base_url.clone(),
                        child,
                    },
                );
                return Ok(OpenCodeRunLease {
                    servers: self.clone(),
                    key: key.to_string(),
                    base_url,
                });
            }
        };
        let _ = child.start_kill();
        let _ = child.wait().await;
        anyhow::bail!(conflict)
    }

    fn take(&self, key: &str) -> Option<RunServer> {
        self.lock().remove(key)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, RunServer>> {
        self.runs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Owns one run's `opencode serve`. Stopping or dropping it kills the process
/// and forgets its base URL.
pub struct OpenCodeRunLease {
    servers: Arc<OpenCodeRunServers>,
    key: String,
    base_url: String,
}

impl OpenCodeRunLease {
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn stop(self) {
        if let Some(mut run) = self.servers.take(&self.key) {
            let _ = run.child.start_kill();
            let _ = run.child.wait().await;
        }
    }
}

impl Drop for OpenCodeRunLease {
    fn drop(&mut self) {
        if let Some(mut run) = self.servers.take(&self.key) {
            let _ = run.child.start_kill();
        }
    }
}

fn free_port(hostname: &str) -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind((hostname, 0))
        .map_err(|err| anyhow::anyhow!("failed to find a free port on {hostname}: {err}"))?;
    Ok(listener.local_addr()?.port())
}

enum Unhealthy {
    Exited(ExitStatus),
    TimedOut,
}

impl Unhealthy {
    fn into_error(self, base_url: &str) -> anyhow::Error {
        match self {
            Self::Exited(status) => anyhow::anyhow!(
                "opencode serve at {base_url} exited with {status} before becoming healthy. \
                 Check ~/.local/share/opencode/log/ for details"
            ),
            Self::TimedOut => anyhow::anyhow!(
                "opencode serve health check timed out after {}s at {base_url}",
                HEALTH_DEADLINE.as_secs()
            ),
        }
    }
}

/// OpenCode can accept a request while still booting and never answer it, so
/// each probe has a short timeout and is retried until the overall deadline.
async fn wait_for_healthy(base_url: &str, child: &mut Child) -> Result<(), Unhealthy> {
    wait_for_healthy_with(child, || async {
        probe_health(base_url).await.unwrap_or(false)
    })
    .await
}

async fn wait_for_healthy_with<F, Fut>(child: &mut Child, mut probe: F) -> Result<(), Unhealthy>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + HEALTH_DEADLINE;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(Unhealthy::Exited(status));
        }
        if probe().await {
            // A healthy answer may come from another run's server that took
            // our port, in which case our child has died of EADDRINUSE.
            return match child.try_wait() {
                Ok(Some(status)) => Err(Unhealthy::Exited(status)),
                _ => Ok(()),
            };
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Unhealthy::TimedOut);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn probe_health(base_url: &str) -> anyhow::Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let resp = client.get(format!("{base_url}/doc")).send().await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_config_registers_coppice_remote_server_without_plaintext_token() {
        let access = McpAccess {
            url: "http://127.0.0.1:5000/mcp".into(),
            token: "secret-run-token".into(),
        };
        let cfg = opencode_run_config(Some(&access));
        assert_eq!(cfg["mcp"]["coppice"]["url"], "http://127.0.0.1:5000/mcp");
        assert_eq!(
            cfg["mcp"]["coppice"]["headers"]["Authorization"],
            "Bearer {env:COPPICE_MCP_TOKEN}"
        );
        assert!(!cfg.to_string().contains("secret-run-token"));
    }

    #[test]
    fn run_config_without_access_has_no_mcp() {
        assert!(opencode_run_config(None).get("mcp").is_none());
    }

    fn sleeper(secs: &str) -> Child {
        tokio::process::Command::new("sleep")
            .arg(secs)
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sleep")
    }

    #[tokio::test]
    async fn healthy_probe_from_another_process_is_not_accepted_once_child_exited() {
        let mut child = sleeper("0.2");
        let result = wait_for_healthy_with(&mut child, || async {
            tokio::time::sleep(Duration::from_millis(800)).await;
            true
        })
        .await;
        assert!(matches!(result, Err(Unhealthy::Exited(_))));
    }

    #[tokio::test]
    async fn healthy_probe_with_running_child_is_accepted() {
        let mut child = sleeper("5");
        let result = wait_for_healthy_with(&mut child, || async { true }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn register_rejects_base_url_owned_by_another_run() {
        let servers = OpenCodeRunServers::new("opencode".into(), "127.0.0.1".into());
        let base_url = "http://127.0.0.1:1".to_string();
        let lease = servers
            .register("run-a", base_url.clone(), sleeper("5"))
            .await
            .expect("first register");

        let err = servers
            .register("run-b", base_url.clone(), sleeper("5"))
            .await
            .err()
            .expect("duplicate base_url must be rejected");
        assert!(err.to_string().contains(&base_url), "{err}");
        assert_eq!(
            servers.base_url("run-a").as_deref(),
            Some(base_url.as_str())
        );
        assert_eq!(servers.base_url("run-b"), None);
        lease.stop().await;
    }
}
