use dashmap::DashMap;
use uuid::Uuid;

use crate::domain::agent::Agent;
use crate::providers::ProviderRegistry;
use crate::sessions::opencode_serve::OpenCodeServeManager;

pub use crate::domain::agent_health::{health_status_to_str, AgentHealthStatus};

#[derive(Debug, Clone)]
pub struct AgentHealthRecord {
    pub status: AgentHealthStatus,
    pub detail: Option<String>,
    pub checked_at: Option<time::OffsetDateTime>,
}

pub struct AgentHealthRegistry {
    inner: DashMap<Uuid, AgentHealthRecord>,
}

impl Default for AgentHealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentHealthRegistry {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    pub fn ensure_agent(&self, agent_id: Uuid) {
        self.inner.entry(agent_id).or_insert(AgentHealthRecord {
            status: AgentHealthStatus::Unknown,
            detail: None,
            checked_at: None,
        });
    }

    pub fn set(&self, agent_id: Uuid, status: AgentHealthStatus, detail: Option<String>) {
        self.inner.insert(
            agent_id,
            AgentHealthRecord {
                status,
                detail,
                checked_at: Some(time::OffsetDateTime::now_utc()),
            },
        );
    }

    pub fn get(&self, agent_id: Uuid) -> AgentHealthRecord {
        self.inner
            .get(&agent_id)
            .map(|e| e.clone())
            .unwrap_or(AgentHealthRecord {
                status: AgentHealthStatus::Unknown,
                detail: None,
                checked_at: None,
            })
    }
}

pub async fn evaluate_agent_health(
    agent: &Agent,
    registry: &ProviderRegistry,
    opencode_serve: Option<&OpenCodeServeManager>,
) -> (AgentHealthStatus, Option<String>) {
    if !registry.has(&agent.provider) {
        return (
            AgentHealthStatus::MissingConfig,
            Some(format!(
                "Provider '{}' is not configured on this server",
                agent.provider
            )),
        );
    }

    match agent.provider.as_str() {
        "mock" => (AgentHealthStatus::Healthy, None),
        "opencode" => {
            let Some(serve) = opencode_serve else {
                return (
                    AgentHealthStatus::Unhealthy,
                    Some("opencode serve is not running".into()),
                );
            };
            match check_opencode_healthy(serve.base_url()).await {
                Ok(()) => (AgentHealthStatus::Healthy, None),
                Err(err) => (AgentHealthStatus::Unhealthy, Some(err.to_string())),
            }
        }
        other => (
            AgentHealthStatus::MissingConfig,
            Some(format!("Unknown provider: {other}")),
        ),
    }
}

async fn check_opencode_healthy(base_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client.get(format!("{base_url}/doc")).send().await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("opencode serve returned {}", resp.status());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_serializes_snake_case() {
        assert_eq!(
            health_status_to_str(AgentHealthStatus::MissingConfig),
            "missing_config"
        );
    }

    #[test]
    fn registry_starts_unknown() {
        let reg = AgentHealthRegistry::new();
        let id = Uuid::new_v4();
        reg.ensure_agent(id);
        assert_eq!(reg.get(id).status, AgentHealthStatus::Unknown);
    }
}
