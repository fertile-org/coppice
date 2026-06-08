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
        let child = tokio::process::Command::new(&config.command)
            .args([
                "serve",
                "--hostname",
                &config.serve_hostname,
                "--port",
                &config.serve_port.to_string(),
            ])
            .spawn()?;
        let base_url = format!("http://{}:{}", config.serve_hostname, config.serve_port);
        wait_for_healthy(&base_url).await?;
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

async fn wait_for_healthy(base_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}/doc");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("opencode serve health check timed out after 30s");
        }
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}
