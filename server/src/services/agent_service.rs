use crate::domain::agent::{Agent, AgentPreset};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct AgentService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent not found")]
    AgentNotFound,
    #[error("preset not found")]
    PresetNotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error("agent cannot be deleted because immutable knowledge revisions reference it")]
    KnowledgeProvenanceConflict,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> AgentService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_presets(&self) -> Result<Vec<AgentPreset>, AgentError> {
        let rows = sqlx::query(
            r#"
            SELECT id, key, role, skills, responsibilities
            FROM agent_presets
            ORDER BY key ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_preset).collect())
    }

    pub async fn list_agents(&self) -> Result<Vec<Agent>, AgentError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, name, role, skills, responsibilities, system_prompt,
                connector, model_provider, model, enabled, preset_source, created_at, updated_at
            FROM agents
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_agent).collect())
    }

    pub async fn get(&self, agent_id: Uuid) -> Result<Agent, AgentError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, role, skills, responsibilities, system_prompt,
                connector, model_provider, model, enabled, preset_source, created_at, updated_at
            FROM agents
            WHERE id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(AgentError::AgentNotFound)?;

        Ok(row_to_agent(&row))
    }

    pub async fn get_preset(&self, preset_id: Uuid) -> Result<AgentPreset, AgentError> {
        let row = sqlx::query(
            r#"
            SELECT id, key, role, skills, responsibilities
            FROM agent_presets
            WHERE id = $1
            "#,
        )
        .bind(preset_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(AgentError::PresetNotFound)?;

        Ok(row_to_preset(&row))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_from_preset(
        &self,
        preset_id: Uuid,
        name: &str,
        system_prompt: &str,
        connector: Option<&str>,
        model_provider: Option<&str>,
        model: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Agent, AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::Validation("name is required".into()));
        }
        if system_prompt.trim().is_empty() {
            return Err(AgentError::Validation("systemPrompt is required".into()));
        }
        if let Some(mp) = model_provider {
            if mp.trim().is_empty() {
                return Err(AgentError::Validation(
                    "modelProvider cannot be empty".into(),
                ));
            }
        }

        let preset = self.get_preset(preset_id).await?;
        self.insert_agent(
            name,
            &preset.role,
            &preset.skills,
            &preset.responsibilities,
            system_prompt,
            connector.unwrap_or("mock"),
            model_provider,
            model,
            enabled.unwrap_or(true),
            Some(&preset.key),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        name: &str,
        role: &str,
        skills: &[String],
        responsibilities: &[String],
        system_prompt: &str,
        connector: Option<&str>,
        model_provider: Option<&str>,
        model: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Agent, AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::Validation("name is required".into()));
        }
        if role.trim().is_empty() {
            return Err(AgentError::Validation("role is required".into()));
        }
        if system_prompt.trim().is_empty() {
            return Err(AgentError::Validation("systemPrompt is required".into()));
        }
        if let Some(mp) = model_provider {
            if mp.trim().is_empty() {
                return Err(AgentError::Validation(
                    "modelProvider cannot be empty".into(),
                ));
            }
        }

        self.insert_agent(
            name,
            role,
            skills,
            responsibilities,
            system_prompt,
            connector.unwrap_or("mock"),
            model_provider,
            model,
            enabled.unwrap_or(true),
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        agent_id: Uuid,
        name: Option<&str>,
        role: Option<&str>,
        skills: Option<&[String]>,
        responsibilities: Option<&[String]>,
        system_prompt: Option<&str>,
        connector: Option<&str>,
        model_provider: Option<&str>,
        model: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Agent, AgentError> {
        if let Some(mp) = model_provider {
            if mp.trim().is_empty() {
                return Err(AgentError::Validation(
                    "modelProvider cannot be empty".into(),
                ));
            }
        }

        let current = self.get(agent_id).await?;
        let name = name.unwrap_or(&current.name);
        let role = role.unwrap_or(&current.role);
        let skills = skills.unwrap_or(&current.skills);
        let responsibilities = responsibilities.unwrap_or(&current.responsibilities);
        let system_prompt = system_prompt.unwrap_or(&current.system_prompt);
        let connector = connector.unwrap_or(&current.connector);
        let model_provider = model_provider.or(current.model_provider.as_deref());
        let model = model.or(current.model.as_deref());
        let enabled = enabled.unwrap_or(current.enabled);

        let row = sqlx::query(
            r#"
            UPDATE agents
            SET
                name = $2,
                role = $3,
                skills = $4,
                responsibilities = $5,
                system_prompt = $6,
                connector = $7,
                model_provider = $8,
                model = $9,
                enabled = $10,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, name, role, skills, responsibilities, system_prompt,
                connector, model_provider, model, enabled, preset_source, created_at, updated_at
            "#,
        )
        .bind(agent_id)
        .bind(name)
        .bind(role)
        .bind(skills)
        .bind(responsibilities)
        .bind(system_prompt)
        .bind(connector)
        .bind(model_provider)
        .bind(model)
        .bind(enabled)
        .fetch_optional(self.pool)
        .await?
        .ok_or(AgentError::AgentNotFound)?;

        Ok(row_to_agent(&row))
    }

    pub async fn delete(&self, agent_id: Uuid) -> Result<(), AgentError> {
        let has_knowledge_provenance: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM knowledge_revisions revision
                LEFT JOIN agent_runs source_run ON source_run.id = revision.source_run_id
                WHERE revision.agent_id = $1 OR source_run.agent_id = $1
            )
            "#,
        )
        .bind(agent_id)
        .fetch_one(self.pool)
        .await?;
        if has_knowledge_provenance {
            return Err(AgentError::KnowledgeProvenanceConflict);
        }

        let result = sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(agent_id)
            .execute(self.pool)
            .await
            .map_err(|error| {
                if matches!(
                    error
                        .as_database_error()
                        .and_then(|database_error| database_error.constraint()),
                    Some(
                        "knowledge_revisions_agent_id_fkey"
                            | "knowledge_revisions_source_run_id_fkey"
                    )
                ) {
                    AgentError::KnowledgeProvenanceConflict
                } else {
                    AgentError::Database(error)
                }
            })?;

        if result.rows_affected() == 0 {
            return Err(AgentError::AgentNotFound);
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_agent(
        &self,
        name: &str,
        role: &str,
        skills: &[String],
        responsibilities: &[String],
        system_prompt: &str,
        connector: &str,
        model_provider: Option<&str>,
        model: Option<&str>,
        enabled: bool,
        preset_source: Option<&str>,
    ) -> Result<Agent, AgentError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt,
                connector, model_provider, model, enabled, preset_source
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING
                id, name, role, skills, responsibilities, system_prompt,
                connector, model_provider, model, enabled, preset_source, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(role)
        .bind(skills)
        .bind(responsibilities)
        .bind(system_prompt)
        .bind(connector)
        .bind(model_provider)
        .bind(model)
        .bind(enabled)
        .bind(preset_source)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_agent(&row))
    }
}

fn row_to_preset(row: &sqlx::postgres::PgRow) -> AgentPreset {
    AgentPreset {
        id: row.get("id"),
        key: row.get("key"),
        role: row.get("role"),
        skills: row.get("skills"),
        responsibilities: row.get("responsibilities"),
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Agent {
    Agent {
        id: row.get("id"),
        name: row.get("name"),
        role: row.get("role"),
        skills: row.get("skills"),
        responsibilities: row.get("responsibilities"),
        system_prompt: row.get("system_prompt"),
        connector: row.get("connector"),
        model_provider: row.get("model_provider"),
        model: row.get("model"),
        enabled: row.get("enabled"),
        preset_source: row.get("preset_source"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_presets_has_ten_entries() {
        let pool = match crate::db::shared_test_pool().await {
            Ok(pool) => pool,
            Err(_) => return,
        };
        crate::db::truncate_test_workspace(&pool).await.ok();

        let service = AgentService::new(&pool);
        let presets = service.list_presets().await.expect("list presets");
        assert_eq!(presets.len(), 10);
    }
}
