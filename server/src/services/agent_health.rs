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
    let Some(models) = registry.models(&agent.connector) else {
        return (
            AgentHealthStatus::MissingConfig,
            Some(missing_connector_detail(&agent.connector)),
        );
    };
    if let Some(ref mp) = agent.model_provider {
        if models.checks_model_provider() && !models.model_providers().contains(mp) {
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

/// Known connectors that are not in the registry are turned off. Anything else
/// keeps the historical "not configured" wording.
pub fn missing_connector_detail(id: &str) -> String {
    match coppice_connectors::get(id) {
        Some(descriptor) if descriptor.id != coppice_connectors::MOCK => {
            crate::connectors_runtime::turned_off_message(descriptor.display_name)
        }
        _ => format!("Connector '{id}' is not configured on this server"),
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

    fn agent(connector: &str, model_provider: Option<&str>) -> Agent {
        let now = time::OffsetDateTime::now_utc();
        Agent {
            id: Uuid::new_v4(),
            name: "Bot".into(),
            role: "Developer".into(),
            responsibilities: vec![],
            system_prompt: "x".into(),
            connector: connector.into(),
            model_provider: model_provider.map(Into::into),
            model: None,
            enabled: true,
            preset_source: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn registry(config: &crate::config::AppConfig) -> ConnectorRegistry {
        ConnectorRegistry::from_config(
            config,
            crate::sessions::opencode_run_server::OpenCodeRunServers::new(
                "opencode".into(),
                "127.0.0.1".into(),
            ),
        )
    }

    #[tokio::test]
    async fn health_disabled_connector_says_turned_off() {
        let mut config = crate::config::AppConfig::load_defaults().expect("config");
        let reg = registry(&config);
        let (status, detail) = evaluate_agent_health(&agent("cursor", None), &reg).await;
        assert_eq!(status, AgentHealthStatus::MissingConfig);
        assert_eq!(
            detail.as_deref(),
            Some(
                "Cursor is turned off. Turn it on in Tools → Connectors, or switch this agent to another connector."
            )
        );
        let (_status, detail) = evaluate_agent_health(&agent("not-a-connector", None), &reg).await;
        assert_eq!(
            detail.as_deref(),
            Some("Connector 'not-a-connector' is not configured on this server")
        );

        config.agent.connectors.cursor.enabled = true;
        config.agent.connectors.cursor.model_providers = vec!["cursor".into()];
        let reg = registry(&config);
        let (status, detail) = evaluate_agent_health(&agent("cursor", Some("x")), &reg).await;
        assert_eq!(status, AgentHealthStatus::MissingConfig);
        assert_eq!(
            detail.as_deref(),
            Some("Model provider 'x' is not configured on this server")
        );
        let (status, detail) = evaluate_agent_health(&agent("cursor", Some("cursor")), &reg).await;
        assert_eq!(status, AgentHealthStatus::Healthy);
        assert_eq!(detail, None);
    }

    #[tokio::test]
    async fn health_mock_ignores_model_provider() {
        let config = crate::config::AppConfig::load_defaults().expect("config");
        let reg = registry(&config);
        let (status, detail) = evaluate_agent_health(&agent("mock", Some("x")), &reg).await;
        assert_eq!(status, AgentHealthStatus::Healthy);
        assert_eq!(detail, None);
    }

    #[test]
    fn registry_starts_unknown() {
        let reg = AgentHealthRegistry::new();
        let id = Uuid::new_v4();
        reg.ensure_agent(id);
        assert_eq!(reg.get(id).status, AgentHealthStatus::Unknown);
    }
}
