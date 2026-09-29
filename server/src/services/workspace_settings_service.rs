use crate::domain::agent::Agent;
use crate::providers::connector_enforces_read_only;
use crate::services::agent_service::{AgentError, AgentService};
use sqlx::PgPool;
use uuid::Uuid;

const COMPACTION_AGENT_FK: &str = "workspace_settings_knowledge_compaction_agent_id_fkey";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceSettingsError {
    #[error("agent not found")]
    AgentNotFound,
    #[error(
        "knowledge compaction requires a read-only capable connector; `{0}` cannot enforce read-only tools"
    )]
    ConnectorNotReadOnly(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct WorkspaceSettingsService<'a> {
    pool: &'a PgPool,
}

impl<'a> WorkspaceSettingsService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// The configured compaction agent id; a missing settings row reads as unset.
    pub async fn knowledge_compaction_agent_id(&self) -> Result<Option<Uuid>, sqlx::Error> {
        let id: Option<Option<Uuid>> = sqlx::query_scalar(
            "SELECT knowledge_compaction_agent_id FROM workspace_settings WHERE id",
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(id.flatten())
    }

    pub async fn knowledge_compaction_agent(
        &self,
    ) -> Result<Option<Agent>, WorkspaceSettingsError> {
        let Some(agent_id) = self.knowledge_compaction_agent_id().await? else {
            return Ok(None);
        };
        match load_agent(self.pool, agent_id).await {
            Ok(agent) => Ok(Some(agent)),
            Err(WorkspaceSettingsError::AgentNotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub async fn set_knowledge_compaction_agent(
        &self,
        agent_id: Option<Uuid>,
        user_id: Uuid,
    ) -> Result<Option<Agent>, WorkspaceSettingsError> {
        let agent = match agent_id {
            Some(agent_id) => {
                let agent = load_agent(self.pool, agent_id).await?;
                if !connector_enforces_read_only(&agent.connector) {
                    return Err(WorkspaceSettingsError::ConnectorNotReadOnly(
                        agent.connector,
                    ));
                }
                Some(agent)
            }
            None => None,
        };
        sqlx::query(
            r#"
            INSERT INTO workspace_settings (id, knowledge_compaction_agent_id, updated_by, updated_at)
            VALUES (TRUE, $1, $2, now())
            ON CONFLICT (id) DO UPDATE
            SET knowledge_compaction_agent_id = EXCLUDED.knowledge_compaction_agent_id,
                updated_by = EXCLUDED.updated_by,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(agent_id)
        .bind(user_id)
        .execute(self.pool)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .and_then(|database_error| database_error.constraint())
                == Some(COMPACTION_AGENT_FK)
            {
                WorkspaceSettingsError::AgentNotFound
            } else {
                WorkspaceSettingsError::Database(error)
            }
        })?;
        Ok(agent)
    }
}

async fn load_agent(pool: &PgPool, agent_id: Uuid) -> Result<Agent, WorkspaceSettingsError> {
    AgentService::new(pool)
        .get(agent_id)
        .await
        .map_err(|error| match error {
            AgentError::Database(error) => WorkspaceSettingsError::Database(error),
            _ => WorkspaceSettingsError::AgentNotFound,
        })
}
