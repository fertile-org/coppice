use crate::domain::agent::{compare_agents_for_board, Agent, AgentPreset, ThinkingLevel};
use crate::services::plugin_service::PluginService;
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
    #[error("agent cannot be deleted because immutable knowledge provenance references it")]
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
            SELECT id, key, role, responsibilities, default_plugins
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
                id, name, role, responsibilities, system_prompt,
                connector, model_provider, model, thinking_level, enabled, preset_source,
                created_at, updated_at
            FROM agents
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        let mut agents: Vec<Agent> = rows.iter().map(row_to_agent).collect();
        agents.sort_by(compare_agents_for_board);
        Ok(agents)
    }

    pub async fn get(&self, agent_id: Uuid) -> Result<Agent, AgentError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, role, responsibilities, system_prompt,
                connector, model_provider, model, thinking_level, enabled, preset_source,
                created_at, updated_at
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
            SELECT id, key, role, responsibilities, default_plugins
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
        thinking_level: Option<&str>,
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
        let connector = resolve_new_connector(connector)?;
        let thinking_level = for_connector(connector, parse_thinking_level(thinking_level)?);
        let agent = self
            .insert_agent(
                name,
                &preset.role,
                &preset.responsibilities,
                system_prompt,
                connector,
                model_provider,
                model,
                thinking_level,
                enabled.unwrap_or(true),
                Some(&preset.key),
            )
            .await?;
        if let Err(err) = PluginService::new(self.pool)
            .apply_preset_defaults(agent.id, &preset.default_plugins)
            .await
        {
            tracing::warn!(agent_id = %agent.id, error = %err, "failed to apply preset default plugins");
        }
        Ok(agent)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        name: &str,
        role: &str,
        responsibilities: &[String],
        system_prompt: &str,
        connector: Option<&str>,
        model_provider: Option<&str>,
        model: Option<&str>,
        thinking_level: Option<&str>,
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

        let connector = resolve_new_connector(connector)?;
        let thinking_level = for_connector(connector, parse_thinking_level(thinking_level)?);
        self.insert_agent(
            name,
            role,
            responsibilities,
            system_prompt,
            connector,
            model_provider,
            model,
            thinking_level,
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
        responsibilities: Option<&[String]>,
        system_prompt: Option<&str>,
        connector: Option<&str>,
        model_provider: Option<Option<&str>>,
        model: Option<Option<&str>>,
        thinking_level: Option<Option<&str>>,
        enabled: Option<bool>,
    ) -> Result<Agent, AgentError> {
        let current = self.get(agent_id).await?;
        let name = name.unwrap_or(&current.name);
        let role = role.unwrap_or(&current.role);
        let responsibilities = responsibilities.unwrap_or(&current.responsibilities);
        let system_prompt = system_prompt.unwrap_or(&current.system_prompt);
        let connector = connector.unwrap_or(&current.connector);
        // `None` keeps the saved value. `Some(None)` and blank text clear it,
        // which is how the form selects "{connector}'s default".
        let model_provider =
            resolve_cleared_text(model_provider, current.model_provider.as_deref());
        let model = resolve_cleared_text(model, current.model.as_deref());
        let thinking_level = match thinking_level {
            None => current.thinking_level,
            Some(value) => parse_thinking_level(value)?,
        };
        // Moving to a connector without the capability drops the level, so
        // no hidden level comes back if the agent later moves back.
        let thinking_level = for_connector(connector, thinking_level);
        let enabled = enabled.unwrap_or(current.enabled);

        let row = sqlx::query(
            r#"
            UPDATE agents
            SET
                name = $2,
                role = $3,
                responsibilities = $4,
                system_prompt = $5,
                connector = $6,
                model_provider = $7,
                model = $8,
                thinking_level = $9,
                enabled = $10,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, name, role, responsibilities, system_prompt,
                connector, model_provider, model, thinking_level, enabled, preset_source,
                created_at, updated_at
            "#,
        )
        .bind(agent_id)
        .bind(name)
        .bind(role)
        .bind(responsibilities)
        .bind(system_prompt)
        .bind(connector)
        .bind(model_provider)
        .bind(model)
        .bind(thinking_level.map(ThinkingLevel::as_str))
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
                SELECT 1 FROM knowledge_revisions revision WHERE revision.agent_id = $1
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
                    Some("knowledge_revisions_agent_id_fkey")
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
        responsibilities: &[String],
        system_prompt: &str,
        connector: &str,
        model_provider: Option<&str>,
        model: Option<&str>,
        thinking_level: Option<ThinkingLevel>,
        enabled: bool,
        preset_source: Option<&str>,
    ) -> Result<Agent, AgentError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, responsibilities, system_prompt,
                connector, model_provider, model, thinking_level, enabled, preset_source
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING
                id, name, role, responsibilities, system_prompt,
                connector, model_provider, model, thinking_level, enabled, preset_source,
                created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(role)
        .bind(responsibilities)
        .bind(system_prompt)
        .bind(connector)
        .bind(model_provider)
        .bind(model)
        .bind(thinking_level.map(ThinkingLevel::as_str))
        .bind(enabled)
        .bind(preset_source)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_agent(&row))
    }
}

/// Blank or missing means "the CLI's default". Anything off the scale is refused.
fn parse_thinking_level(value: Option<&str>) -> Result<Option<ThinkingLevel>, AgentError> {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        None => Ok(None),
        Some(text) => ThinkingLevel::parse(text).map(Some).ok_or_else(|| {
            AgentError::Validation(format!(
                "thinkingLevel must be one of low, medium, high, max (got `{text}`)"
            ))
        }),
    }
}

/// A level is stored only for a connector that maps it to a flag. Elsewhere it
/// would do nothing and the form could not show or clear it.
fn for_connector(connector: &str, level: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    level.filter(|_| coppice_connectors::get(connector).is_some_and(|d| d.takes_thinking_level()))
}

/// Dev/test builds default a missing connector to mock. Release builds refuse
/// it: the mock provider is not compiled in, so it must not be stored either.
/// Absent field keeps `current`. A present blank or null clears it.
fn resolve_cleared_text<'a>(
    incoming: Option<Option<&'a str>>,
    current: Option<&'a str>,
) -> Option<&'a str> {
    match incoming {
        None => current,
        Some(value) => value.map(str::trim).filter(|text| !text.is_empty()),
    }
}

fn resolve_new_connector(connector: Option<&str>) -> Result<&str, AgentError> {
    #[cfg(feature = "mock-provider")]
    let connector = connector.unwrap_or(coppice_connectors::MOCK);
    #[cfg(not(feature = "mock-provider"))]
    let connector =
        connector.ok_or_else(|| AgentError::Validation("connector is required".into()))?;
    #[cfg(not(feature = "mock-provider"))]
    if connector == coppice_connectors::MOCK {
        return Err(AgentError::Validation(
            "connector `mock` is not available in this build".into(),
        ));
    }
    Ok(connector)
}

fn row_to_preset(row: &sqlx::postgres::PgRow) -> AgentPreset {
    AgentPreset {
        id: row.get("id"),
        key: row.get("key"),
        role: row.get("role"),
        responsibilities: row.get("responsibilities"),
        default_plugins: row.get("default_plugins"),
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Agent {
    Agent {
        id: row.get("id"),
        name: row.get("name"),
        role: row.get("role"),
        responsibilities: row.get("responsibilities"),
        system_prompt: row.get("system_prompt"),
        connector: row.get("connector"),
        model_provider: row.get("model_provider"),
        model: row.get("model"),
        thinking_level: stored_thinking_level(row),
        enabled: row.get("enabled"),
        preset_source: row.get("preset_source"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// The column's CHECK keeps it on the scale. A value outside it (a level added
/// in SQL before this code knows it) reads as the CLI default, and the warning
/// says so, because the next save from the form overwrites it.
fn stored_thinking_level(row: &sqlx::postgres::PgRow) -> Option<ThinkingLevel> {
    let stored = row.get::<Option<String>, _>("thinking_level")?;
    let level = ThinkingLevel::parse(&stored);
    if level.is_none() {
        let agent_id: Uuid = row.get("id");
        tracing::warn!(
            %agent_id,
            stored,
            "agent has an unknown thinking level; using the connector's default"
        );
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_the_connector_default_clears_a_saved_model() {
        assert_eq!(resolve_cleared_text(Some(None), Some("auto")), None);
        assert_eq!(resolve_cleared_text(Some(Some("")), Some("auto")), None);
        assert_eq!(resolve_cleared_text(Some(Some("  ")), Some("cursor")), None);
        assert_eq!(
            resolve_cleared_text(None, Some("auto")),
            Some("auto"),
            "an omitted field keeps the saved model"
        );
        assert_eq!(
            resolve_cleared_text(Some(Some(" composer-2.5 ")), None),
            Some("composer-2.5")
        );
    }

    #[tokio::test]
    async fn update_clears_saved_model_and_provider() {
        let pool = match crate::db::shared_test_pool().await {
            Ok(pool) => pool,
            Err(_) => return,
        };
        crate::db::truncate_test_workspace(&pool).await.ok();
        let service = AgentService::new(&pool);
        let created = service
            .create(
                "Builder",
                "Developer",
                &[],
                "You build.",
                Some("cursor"),
                Some("cursor"),
                Some("auto"),
                Some("high"),
                Some(true),
            )
            .await
            .expect("create");
        assert_eq!(created.model.as_deref(), Some("auto"));
        // Cursor has no thinking level capability, so the level is not stored.
        assert!(created.thinking_level.is_none());
        assert_eq!(created.model_provider.as_deref(), Some("cursor"));

        let cleared = service
            .update(
                created.id,
                None,
                None,
                None,
                None,
                None,
                Some(None),
                Some(None),
                Some(None),
                None,
            )
            .await
            .expect("clear");
        assert!(cleared.model.is_none());
        assert!(cleared.model_provider.is_none());
        assert!(cleared.thinking_level.is_none());

        let kept = service
            .update(
                created.id,
                Some("Renamed"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("rename");
        assert_eq!(kept.name, "Renamed");
        assert!(kept.model.is_none());
    }

    #[cfg(not(feature = "mock-provider"))]
    #[test]
    fn release_build_rejects_mock_connector() {
        let missing = resolve_new_connector(None).expect_err("connector required");
        assert!(missing.to_string().contains("required"), "{missing}");
        let mock = resolve_new_connector(Some(coppice_connectors::MOCK)).expect_err("no mock");
        assert!(mock.to_string().contains("not available"), "{mock}");
    }

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
    #[test]
    fn thinking_level_parses_the_shared_scale() {
        assert_eq!(parse_thinking_level(None).unwrap(), None);
        assert_eq!(parse_thinking_level(Some("  ")).unwrap(), None);
        assert_eq!(
            parse_thinking_level(Some(" max ")).unwrap(),
            Some(ThinkingLevel::Max)
        );
        for level in ["low", "medium", "high", "max"] {
            let parsed = parse_thinking_level(Some(level)).unwrap().unwrap();
            assert_eq!(parsed.as_str(), level);
        }
        assert!(matches!(
            parse_thinking_level(Some("xhigh")),
            Err(AgentError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn update_sets_keeps_and_rejects_thinking_level() {
        let pool = match crate::db::shared_test_pool().await {
            Ok(pool) => pool,
            Err(_) => return,
        };
        crate::db::truncate_test_workspace(&pool).await.ok();
        let service = AgentService::new(&pool);
        let created = service
            .create(
                "Thinker",
                "Developer",
                &[],
                "You think.",
                Some("claude-code"),
                None,
                None,
                None,
                Some(true),
            )
            .await
            .expect("create");
        assert!(created.thinking_level.is_none());

        let set = service
            .update(
                created.id,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(Some("low")),
                None,
            )
            .await
            .expect("set");
        assert_eq!(set.thinking_level, Some(ThinkingLevel::Low));

        let kept = service
            .update(
                created.id,
                Some("Renamed"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .expect("keep");
        assert_eq!(kept.thinking_level, Some(ThinkingLevel::Low));

        let rejected = service
            .update(
                created.id,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(Some("ultra")),
                None,
            )
            .await;
        assert!(matches!(rejected, Err(AgentError::Validation(_))));
        let after = service.get(created.id).await.expect("get");
        assert_eq!(after.thinking_level, Some(ThinkingLevel::Low));

        // A connector without the capability drops the level, even one sent
        // in the same request, and it stays gone after moving back.
        let moved = service
            .update(
                created.id,
                None,
                None,
                None,
                None,
                Some(coppice_connectors::CODEX),
                None,
                None,
                Some(Some("max")),
                None,
            )
            .await
            .expect("move to codex");
        assert!(moved.thinking_level.is_none());
        let back = service
            .update(
                created.id,
                None,
                None,
                None,
                None,
                Some(coppice_connectors::CLAUDE_CODE),
                None,
                None,
                None,
                None,
            )
            .await
            .expect("move back");
        assert!(back.thinking_level.is_none());
    }
}
