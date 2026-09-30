use dashmap::DashMap;
use uuid::Uuid;

use crate::domain::agent::Agent;
use crate::providers::ConnectorRegistry;

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
    registry: &ConnectorRegistry,
) -> (AgentHealthStatus, Option<String>) {
    if !registry.has(&agent.connector) {
        return (
            AgentHealthStatus::MissingConfig,
            Some(format!(
                "Connector '{}' is not configured on this server",
                agent.connector
            )),
        );
    }

    match agent.connector.as_str() {
        "mock" => (AgentHealthStatus::Healthy, None),
        "claude-code" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("claude-code", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
            (AgentHealthStatus::Healthy, None)
        }
        "codex" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("codex", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
            (AgentHealthStatus::Healthy, None)
        }
        "kilo-code" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("kilo-code", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
            (AgentHealthStatus::Healthy, None)
        }
        "cursor" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("cursor", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
            (AgentHealthStatus::Healthy, None)
        }
        "opencode" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("opencode", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
            (AgentHealthStatus::Healthy, None)
        }
        other => (
            AgentHealthStatus::MissingConfig,
            Some(format!("Unknown connector: {other}")),
        ),
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
