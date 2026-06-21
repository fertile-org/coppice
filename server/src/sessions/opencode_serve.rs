use coppice_config::OpenCodeProviderConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::Mutex;

pub struct OpenCodeServeManager {
    child: Mutex<Option<Child>>,
    base_url: String,
}

impl OpenCodeServeManager {
    pub async fn start(config: &OpenCodeProviderConfig) -> anyhow::Result<Arc<Self>> {
        let base_url = format!("http://{}:{}", config.serve_hostname, config.serve_port);

        if is_healthy(&base_url).await {
            tracing::info!(%base_url, "reusing existing opencode serve");
            return Ok(Arc::new(Self {
                child: Mutex::new(None),
                base_url,
            }));
        }

        let mut child = tokio::process::Command::new(&config.command)
            .args([
                "serve",
                "--hostname",
                &config.serve_hostname,
                "--port",
                &config.serve_port.to_string(),
            ])
            .spawn()?;

        if let Err(err) = wait_for_healthy(&base_url, Some(&mut child)).await {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(err);
        }

        Ok(Arc::new(Self {
            child: Mutex::new(Some(child)),
            base_url,
        }))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn shutdown(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }
}

async fn is_healthy(base_url: &str) -> bool {
    probe_health(base_url).await.unwrap_or(false)
}

async fn wait_for_healthy(base_url: &str, mut child: Option<&mut Child>) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "opencode serve health check timed out after 30s at {base_url}. \
                 If port is already in use, stop the other opencode serve process or change \
                 agent.connectors.opencode.serve_port in config.toml"
            );
        }

        if let Some(child) = child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                anyhow::bail!(
                    "opencode serve exited with {status} before becoming healthy. \
                     Check ~/.local/share/opencode/log/ for details. \
                     If port is already in use, stop the other opencode serve or change serve_port"
                );
            }
        }

        if is_healthy(base_url).await {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn probe_health(base_url: &str) -> anyhow::Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let url = format!("{base_url}/doc");
    let resp = client.get(&url).send().await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_from_config() {
        let config = OpenCodeProviderConfig {
            enabled: true,
            command: "opencode".into(),
            serve_hostname: "127.0.0.1".into(),
            serve_port: 4096,
            run_timeout_secs: 1800,
            model_providers: vec![],
        };
        let url = format!(
            "http://{}:{}",
            config.serve_hostname, config.serve_port
        );
        assert_eq!(url, "http://127.0.0.1:4096");
    }
}
