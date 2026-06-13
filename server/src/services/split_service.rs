use std::collections::HashMap;

use crate::config::WorkflowConfig;
use crate::domain::agent::Agent;
use crate::domain::slug::slugify;
use crate::domain::ticket::{status_to_str, Ticket};
use crate::domain::workflow::{
    PendingRecommendation, PendingSplitRecommendation, SplitTicketSpec,
};
use crate::services::ticket_service::TicketWithDisplay;
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::result_contract::merge_ticket_description;
use crate::services::ticket_service::{TicketError, TicketService};
use sqlx::PgPool;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub struct SplitService<'a> {
    pool: &'a PgPool,
    workflow: &'a WorkflowConfig,
}

#[derive(Debug, Clone)]
pub enum ApplySplitOutcome {
    Pending,
    Created(Vec<Ticket>),
}

#[derive(Debug, thiserror::Error)]
pub enum SplitError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Agent(#[from] AgentError),
}

impl<'a> SplitService<'a> {
    pub fn new(pool: &'a PgPool, workflow: &'a WorkflowConfig) -> Self {
        Self { pool, workflow }
    }

    pub async fn apply_splits(
        &self,
        parent: &Ticket,
        splits: &[SplitTicketSpec],
        recommended_by: Uuid,
        auto_split: bool,
    ) -> Result<ApplySplitOutcome, SplitError> {
        if splits.is_empty() {
            return Ok(ApplySplitOutcome::Pending);
        }

        if auto_split {
            let children = self
                .create_child_tickets(parent, splits, recommended_by)
                .await?;
            return Ok(ApplySplitOutcome::Created(children));
        }

        let recommended_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new());
        let pending = PendingSplitRecommendation {
            recommended_by_agent_id: recommended_by,
            recommended_at,
            splits: splits.to_vec(),
        };
        TicketService::new(self.pool)
            .set_pending_split_recommendation(parent.id, pending)
            .await?;

        Ok(ApplySplitOutcome::Pending)
    }

    pub async fn create_child_tickets(
        &self,
        parent: &Ticket,
        splits: &[SplitTicketSpec],
        recommended_by: Uuid,
    ) -> Result<Vec<Ticket>, SplitError> {
        let agents = AgentService::new(self.pool).list_agents().await?;
        let agent_ids = build_agent_key_map(&agents);
        let ticket_svc = TicketService::new(self.pool);
        let mut children = Vec::with_capacity(splits.len());

        for spec in splits {
            let description = merge_ticket_description(
                "",
                Some(spec.description.as_str()),
                spec.acceptance_criteria.as_deref(),
            )
            .unwrap_or_else(|| spec.description.clone());

            let created = ticket_svc
                .create_child(parent, &spec.title, &description, recommended_by)
                .await?;
            let mut child = created.ticket;

            if let Some(ref assign_key) = spec.assign_to {
                let key = assign_key.trim();
                if key.is_empty() {
                    continue;
                }
                let auto_assign = self
                    .workflow
                    .auto_assign
                    .effective(status_to_str(child.status));
                if let Some(&agent_id) = agent_ids.get(key) {
                    if auto_assign {
                        let updated = ticket_svc.assign_agent(child.id, Some(agent_id)).await?;
                        child = updated.ticket;
                    } else {
                        let recommended_at = OffsetDateTime::now_utc()
                            .format(&Rfc3339)
                            .unwrap_or_else(|_| String::new());
                        let updated = ticket_svc
                            .apply_workflow_update(
                                child.id,
                                None,
                                None,
                                None,
                                None,
                                Some(Some(PendingRecommendation {
                                    recommended_agent_key: key.to_string(),
                                    recommended_by_agent_id: recommended_by,
                                    recommended_at,
                                    summary: None,
                                })),
                                0,
                            )
                            .await?;
                        child = updated.ticket;
                    }
                }
            }

            children.push(child);
        }

        Ok(children)
    }

    pub async fn approve_splits(&self, ticket_id: Uuid) -> Result<Vec<Ticket>, SplitError> {
        let ticket_svc = TicketService::new(self.pool);
        let parent = ticket_svc.get(ticket_id).await?;

        let pending_value = parent
            .ticket
            .pending_split_recommendation
            .as_ref()
            .ok_or_else(|| SplitError::Validation("no pending split recommendation".into()))?
            .clone();

        let pending: PendingSplitRecommendation = serde_json::from_value(pending_value).map_err(
            |e| SplitError::Validation(format!("invalid pending split recommendation: {e}")),
        )?;

        let children = self
            .create_child_tickets(
                &parent.ticket,
                &pending.splits,
                pending.recommended_by_agent_id,
            )
            .await?;

        ticket_svc
            .clear_pending_split_recommendation(ticket_id)
            .await?;

        Ok(children)
    }

    pub async fn dismiss_splits(&self, ticket_id: Uuid) -> Result<TicketWithDisplay, SplitError> {
        let ticket_svc = TicketService::new(self.pool);
        let parent = ticket_svc.get(ticket_id).await?;

        if parent.ticket.pending_split_recommendation.is_none() {
            return Err(SplitError::Validation(
                "no pending split recommendation".into(),
            ));
        }

        ticket_svc
            .clear_pending_split_recommendation(ticket_id)
            .await
            .map_err(SplitError::from)
    }

    pub async fn list_children(
        &self,
        parent_ticket_id: Uuid,
    ) -> Result<Vec<TicketWithDisplay>, SplitError> {
        TicketService::new(self.pool)
            .list_children(parent_ticket_id)
            .await
            .map_err(SplitError::from)
    }
}

fn build_agent_key_map(agents: &[Agent]) -> HashMap<String, Uuid> {
    let mut ids = HashMap::new();

    for agent in agents {
        if !agent.enabled {
            continue;
        }
        if let Some(ref preset) = agent.preset_source {
            ids.insert(preset.clone(), agent.id);
        }
        ids.insert(slugify(&agent.name), agent.id);
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::substatus::TicketStatus;
    use coppice_config::WorkflowConfig;
    use sqlx::PgPool;

    async fn test_pool() -> Option<PgPool> {
        let pool = crate::db::shared_test_pool().await.ok()?;
        crate::db::truncate_test_workspace(&pool).await.ok()?;
        Some(pool)
    }

    #[tokio::test]
    async fn apply_splits_pending_sets_json_no_children() {
        let Some(pool) = test_pool().await else {
            return;
        };

        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind("split project")
            .bind(format!("split-{}", project_id))
            .execute(&pool)
            .await
            .expect("insert project");

        let pm_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(pm_agent_id)
        .bind("PM Agent")
        .bind("PM")
        .bind("prompt")
        .bind("mock")
        .bind("pm")
        .execute(&pool)
        .await
        .expect("insert pm agent");

        let parent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, project_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(parent_id)
        .bind(project_id)
        .bind("Epic ticket")
        .bind("backlog")
        .bind("test")
        .bind(pm_agent_id)
        .execute(&pool)
        .await
        .expect("insert parent ticket");

        let parent = TicketService::new(&pool)
            .get(parent_id)
            .await
            .expect("load parent")
            .ticket;

        let splits = vec![
            SplitTicketSpec {
                title: "Child A".into(),
                description: "First deliverable".into(),
                acceptance_criteria: Some("- Done when A works".into()),
                assign_to: None,
            },
            SplitTicketSpec {
                title: "Child B".into(),
                description: "Second deliverable".into(),
                acceptance_criteria: None,
                assign_to: Some("pm".into()),
            },
        ];

        let workflow = WorkflowConfig::default();
        let outcome = SplitService::new(&pool, &workflow)
            .apply_splits(&parent, &splits, pm_agent_id, false)
            .await
            .expect("apply splits");

        assert!(matches!(outcome, ApplySplitOutcome::Pending));

        let updated = TicketService::new(&pool)
            .get(parent_id)
            .await
            .expect("reload parent");
        assert!(updated.ticket.pending_split_recommendation.is_some());

        let child_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tickets WHERE parent_ticket_id = $1",
        )
        .bind(parent_id)
        .fetch_one(&pool)
        .await
        .expect("count children");
        assert_eq!(child_count, 0);
    }

    #[tokio::test]
    async fn apply_splits_auto_creates_children_no_pending() {
        let Some(pool) = test_pool().await else {
            return;
        };

        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind("split auto project")
            .bind(format!("split-auto-apply-{}", project_id))
            .execute(&pool)
            .await
            .expect("insert project");

        let pm_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(pm_agent_id)
        .bind("PM Agent")
        .bind("PM")
        .bind("prompt")
        .bind("mock")
        .bind("pm")
        .execute(&pool)
        .await
        .expect("insert pm agent");

        let parent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, project_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(parent_id)
        .bind(project_id)
        .bind("Epic ticket")
        .bind("backlog")
        .bind("test")
        .bind(pm_agent_id)
        .execute(&pool)
        .await
        .expect("insert parent ticket");

        let parent = TicketService::new(&pool)
            .get(parent_id)
            .await
            .expect("load parent")
            .ticket;

        let splits = vec![
            SplitTicketSpec {
                title: "Child A".into(),
                description: "First deliverable".into(),
                acceptance_criteria: None,
                assign_to: None,
            },
            SplitTicketSpec {
                title: "Child B".into(),
                description: "Second deliverable".into(),
                acceptance_criteria: None,
                assign_to: None,
            },
        ];

        let workflow = WorkflowConfig::default();
        let outcome = SplitService::new(&pool, &workflow)
            .apply_splits(&parent, &splits, pm_agent_id, true)
            .await
            .expect("apply splits");

        let children = match outcome {
            ApplySplitOutcome::Created(children) => children,
            ApplySplitOutcome::Pending => panic!("expected Created outcome"),
        };
        assert_eq!(children.len(), 2);
        assert!(children
            .iter()
            .all(|child| child.parent_ticket_id == Some(parent_id)));

        let updated = TicketService::new(&pool)
            .get(parent_id)
            .await
            .expect("reload parent");
        assert!(updated.ticket.pending_split_recommendation.is_none());
    }

    #[tokio::test]
    async fn create_child_tickets_merges_description_and_assigns() {
        let Some(pool) = test_pool().await else {
            return;
        };

        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind("split auto project")
            .bind(format!("split-auto-{}", project_id))
            .execute(&pool)
            .await
            .expect("insert project");

        let pm_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(pm_agent_id)
        .bind("PM Agent")
        .bind("PM")
        .bind("prompt")
        .bind("mock")
        .bind("pm")
        .execute(&pool)
        .await
        .expect("insert pm agent");

        let engineer_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(engineer_agent_id)
        .bind("Backend Engineer")
        .bind("Backend Engineer")
        .bind("prompt")
        .bind("mock")
        .bind("backend_engineer")
        .execute(&pool)
        .await
        .expect("insert engineer agent");

        let parent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, project_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(parent_id)
        .bind(project_id)
        .bind("Epic ticket")
        .bind("backlog")
        .bind("test")
        .bind(pm_agent_id)
        .execute(&pool)
        .await
        .expect("insert parent ticket");

        let parent = TicketService::new(&pool)
            .get(parent_id)
            .await
            .expect("load parent")
            .ticket;

        let workflow = WorkflowConfig {
            auto_assign: coppice_config::AutoAssignConfig {
                default: true,
                backlog: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };

        let splits = vec![SplitTicketSpec {
            title: "Retry logic".into(),
            description: "Add exponential backoff".into(),
            acceptance_criteria: Some("- Retries up to 3 times".into()),
            assign_to: Some("backend_engineer".into()),
        }];

        let children = SplitService::new(&pool, &workflow)
            .create_child_tickets(&parent, &splits, pm_agent_id)
            .await
            .expect("create children");

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].parent_ticket_id, Some(parent_id));
        assert_eq!(children[0].status, TicketStatus::Backlog);
        assert_eq!(children[0].assignee_agent_id, Some(engineer_agent_id));
        assert!(children[0].description.contains("Acceptance criteria"));
    }
}
