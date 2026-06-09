use dashmap::DashMap;
use uuid::Uuid;

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
