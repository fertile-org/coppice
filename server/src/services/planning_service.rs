//! Planning runs and the Plan Review gate.
//!
//! A human move to Ready starts one `plan_ticket` run. The plan is a ticket
//! comment. Implementation starts only after that plan is approved for the
//! ticket's current content version, unless planning is skipped.

use crate::copy::plan::{
    self, format_plan, ASK_COMMENT_REQUIRED, ASK_ONLY_FROM_PLAN_REVIEW,
    APPROVE_ONLY_FROM_PLAN_REVIEW, IMPLEMENTATION_NOT_STARTED, NO_PLANNER, NO_PLAN_YET,
    PLAN_ALREADY_RUNNING, PLAN_APPROVED_NOTE, PLAN_STALE,
};
use crate::domain::comment::{AuthorType, Comment, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::substatus::TicketStatus;
use crate::domain::workflow::{is_pm_identity, JOB_TYPE_PLAN_TICKET};
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketService};
use crate::domain::agent::Agent;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub use crate::domain::workflow::JOB_TYPE_PLAN_TICKET as PLAN_JOB;

#[derive(Debug)]
pub enum StartOutcome {
    Started(Uuid),
    AlreadyRunning(Uuid),
    Skipped,
    NoPlanner,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<TicketError> for PlanError {
    fn from(err: TicketError) -> Self {
        match err {
            TicketError::Validation(msg) => PlanError::Validation(msg),
            TicketError::Database(err) => PlanError::Database(err),
            other => PlanError::Validation(other.to_string()),
        }
    }
}

impl From<crate::services::comment_service::CommentError> for PlanError {
    fn from(err: crate::services::comment_service::CommentError) -> Self {
        match err {
            crate::services::comment_service::CommentError::Validation(msg) => {
                PlanError::Validation(msg)
            }
            crate::services::comment_service::CommentError::Database(err) => {
                PlanError::Database(err)
            }
            other => PlanError::Validation(other.to_string()),
        }
    }
}

pub struct ApprovedPlan {
    pub comment_id: Uuid,
    pub markdown: String,
    pub steps: Vec<String>,
}

pub struct PlanningService<'a> {
    pool: &'a PgPool,
}

impl<'a> PlanningService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Start planning after a human moves the ticket to Ready.
    /// A second call while a planning run is queued or running is a no-op.
    pub async fn start_for_ready(&self, ticket_id: Uuid) -> Result<StartOutcome, PlanError> {
        self.start(ticket_id, None).await
    }

    async fn start(
        &self,
        ticket_id: Uuid,
        trigger_comment_id: Option<Uuid>,
    ) -> Result<StartOutcome, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.skip_planning {
            return Ok(StartOutcome::Skipped);
        }
        if let Some(existing) = self.active_plan_run(ticket_id).await? {
            return Ok(StartOutcome::AlreadyRunning(existing));
        }

        let agents = AgentService::new(self.pool).list_agents().await.map_err(|err| {
            PlanError::Validation(err.to_string())
        })?;
        let Some(planner) = choose_planner(&agents, ticket.ticket.assignee_agent_id) else {
            CommentService::new(self.pool)
                .create(
                    ticket_id,
                    AuthorType::System,
                    None,
                    NO_PLANNER,
                    CommentIntent::SystemEvent,
                    &[],
                    &[],
                )
                .await?;
            return Ok(StartOutcome::NoPlanner);
        };

        let run_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(planner.id)
        .bind(JOB_TYPE_PLAN_TICKET)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::Full.as_str())
        .bind(trigger_comment_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = inserted else {
            tx.rollback().await?;
            if let Some(existing) = self.active_plan_run(ticket_id).await? {
                return Ok(StartOutcome::AlreadyRunning(existing));
            }
            return Ok(StartOutcome::AlreadyRunning(run_id));
        };
        let inserted_id: Uuid = row.get("id");

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(inserted_id)
        .bind(JOB_TYPE_PLAN_TICKET)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(StartOutcome::Started(inserted_id))
    }

    /// Post the plan comment and move a Ready ticket to Plan Review.
    pub async fn record_plan(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
        summary: &str,
    ) -> Result<Comment, PlanError> {
        let body = format_plan(summary);
        let comment = CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Agent,
                Some(agent_id),
                &body,
                CommentIntent::Plan,
                &[],
                &[],
            )
            .await?;
        self.bind_plan_comment(ticket_id, comment.id).await?;
        CommentService::new(self.pool).get(comment.id).await.map_err(Into::into)
    }

    /// Stamp the newest plan comment and move Ready into Plan Review.
    pub async fn bind_latest_plan(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let Some(comment) = self.latest_plan_comment(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        self.bind_plan_comment(ticket_id, comment.id).await
    }

    async fn bind_plan_comment(&self, ticket_id: Uuid, comment_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        sqlx::query(
            "UPDATE ticket_comments SET plan_content_version = $2 WHERE id = $1",
        )
        .bind(comment_id)
        .bind(ticket.ticket.content_version)
        .execute(self.pool)
        .await?;
        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_comment_id = NULL,
                approved_plan_content_version = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .execute(self.pool)
        .await?;

        if matches!(
            ticket.ticket.status,
            TicketStatus::Ready | TicketStatus::PlanReview
        ) && ticket.ticket.status != TicketStatus::PlanReview
        {
            TicketService::new(self.pool)
                .update_status(ticket_id, TicketStatus::PlanReview, None, None)
                .await?;
        }
        Ok(())
    }

    pub async fn approve(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::PlanReview {
            return Err(PlanError::Validation(APPROVE_ONLY_FROM_PLAN_REVIEW.into()));
        }
        let Some(plan) = self.latest_plan_comment(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        if plan.plan_content_version != Some(ticket.ticket.content_version) {
            return Err(PlanError::Validation(PLAN_STALE.into()));
        }

        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_comment_id = $2,
                approved_plan_content_version = $3,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(plan.id)
        .bind(ticket.ticket.content_version)
        .execute(self.pool)
        .await?;

        TicketService::new(self.pool)
            .update_status(ticket_id, TicketStatus::InProgress, None, None)
            .await?;

        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::System,
                None,
                PLAN_APPROVED_NOTE,
                CommentIntent::SystemEvent,
                &[],
                &[],
            )
            .await?;

        self.start_implementation(ticket_id).await?;
        Ok(())
    }

    pub async fn ask_for_changes(&self, ticket_id: Uuid, body: &str) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::PlanReview {
            return Err(PlanError::Validation(ASK_ONLY_FROM_PLAN_REVIEW.into()));
        }
        if body.trim().is_empty() {
            return Err(PlanError::Validation(ASK_COMMENT_REQUIRED.into()));
        }
        if self.active_plan_run(ticket_id).await?.is_some() {
            return Err(PlanError::Validation(PLAN_ALREADY_RUNNING.into()));
        }

        let comment = CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Human,
                None,
                body.trim(),
                CommentIntent::ReviewFeedback,
                &[],
                &[],
            )
            .await?;

        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_comment_id = NULL,
                approved_plan_content_version = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .execute(self.pool)
        .await?;

        self.start(ticket_id, Some(comment.id)).await?;
        Ok(())
    }

    pub async fn approved_plan(&self, ticket_id: Uuid) -> Result<Option<ApprovedPlan>, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if !TicketService::new(self.pool)
            .approved_plan_is_current(&ticket.ticket)
            .await?
        {
            return Ok(None);
        }
        let Some(comment_id) = ticket.ticket.approved_plan_comment_id else {
            return Ok(None);
        };
        let comment = CommentService::new(self.pool)
            .get(comment_id)
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        Ok(Some(ApprovedPlan {
            comment_id: comment.id,
            steps: plan::checklist_steps(&comment.body),
            markdown: comment.body,
        }))
    }

    async fn start_implementation(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.assignee_agent_id.is_none() || ticket.ticket.repo_id.is_none() {
            CommentService::new(self.pool)
                .create(
                    ticket_id,
                    AuthorType::System,
                    None,
                    IMPLEMENTATION_NOT_STARTED,
                    CommentIntent::SystemEvent,
                    &[],
                    &[],
                )
                .await?;
            return Ok(());
        }
        match RunService::new(self.pool).start_run(ticket_id).await {
            Ok(_) | Err(RunError::ActiveRunExists) => Ok(()),
            Err(RunError::Validation(_)) => {
                CommentService::new(self.pool)
                    .create(
                        ticket_id,
                        AuthorType::System,
                        None,
                        IMPLEMENTATION_NOT_STARTED,
                        CommentIntent::SystemEvent,
                        &[],
                        &[],
                    )
                    .await?;
                Ok(())
            }
            Err(RunError::Database(err)) => Err(PlanError::Database(err)),
            Err(RunError::NotFound) => Ok(()),
        }
    }

    async fn active_plan_run(&self, ticket_id: Uuid) -> Result<Option<Uuid>, PlanError> {
        let id = sqlx::query_scalar(
            r#"
            SELECT id FROM agent_runs
            WHERE ticket_id = $1
              AND job_type = $2
              AND status IN ('queued', 'running')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(ticket_id)
        .bind(JOB_TYPE_PLAN_TICKET)
        .fetch_optional(self.pool)
        .await?;
        Ok(id)
    }

    pub async fn latest_plan_comment(&self, ticket_id: Uuid) -> Result<Option<Comment>, PlanError> {
        let row = sqlx::query(
            r#"
            SELECT id
            FROM ticket_comments
            WHERE ticket_id = $1 AND intent = 'plan'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id: Uuid = row.get("id");
        CommentService::new(self.pool)
            .get(id)
            .await
            .map(Some)
            .map_err(Into::into)
    }
}

/// PM-role agent first (the assignee when they are a PM, otherwise the oldest
/// enabled PM). Otherwise the assignee. Otherwise nobody.
pub fn choose_planner(agents: &[Agent], assignee_id: Option<Uuid>) -> Option<&Agent> {
    let enabled: Vec<&Agent> = agents.iter().filter(|agent| agent.enabled).collect();
    if let Some(assignee_id) = assignee_id {
        if let Some(assignee) = enabled.iter().copied().find(|agent| agent.id == assignee_id) {
            if is_pm_agent(assignee) {
                return Some(assignee);
            }
        }
    }
    let mut pms: Vec<&Agent> = enabled.iter().copied().filter(|agent| is_pm_agent(agent)).collect();
    pms.sort_by_key(|agent| agent.created_at);
    if let Some(pm) = pms.first() {
        return Some(*pm);
    }
    assignee_id.and_then(|id| enabled.into_iter().find(|agent| agent.id == id))
}

fn is_pm_agent(agent: &Agent) -> bool {
    let key = agent.preset_source.as_deref().unwrap_or("");
    is_pm_identity(key, &agent.role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn agent(id: u128, role: &str, key: &str, enabled: bool, created_secs: i64) -> Agent {
        Agent {
            id: Uuid::from_u128(id),
            name: role.into(),
            role: role.into(),
            skills: vec![],
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "claude-code".into(),
            model_provider: None,
            model: None,
            enabled,
            preset_source: Some(key.into()),
            created_at: OffsetDateTime::from_unix_timestamp(created_secs).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(created_secs).unwrap(),
        }
    }

    #[test]
    fn planner_prefers_the_pm_over_the_assignee() {
        let pm = agent(1, "PM", "pm", true, 20);
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        let agents = [engineer.clone(), pm.clone()];
        let chosen = choose_planner(&agents, Some(engineer.id));
        assert_eq!(chosen.map(|agent| agent.id), Some(pm.id));
    }

    #[test]
    fn planner_uses_the_assignee_when_they_are_the_pm() {
        let pm = agent(1, "PM", "pm", true, 20);
        let other = agent(3, "Product Manager", "other", true, 5);
        let agents = [other.clone(), pm.clone()];
        let chosen = choose_planner(&agents, Some(pm.id));
        assert_eq!(chosen.map(|agent| agent.id), Some(pm.id));
    }

    #[test]
    fn planner_falls_back_to_the_assignee() {
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        let agents = [engineer.clone()];
        let chosen = choose_planner(&agents, Some(engineer.id));
        assert_eq!(chosen.map(|agent| agent.id), Some(engineer.id));
    }

    #[test]
    fn planner_is_none_without_a_pm_or_assignee() {
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        assert!(choose_planner(&[engineer], None).is_none());
        let disabled = agent(1, "PM", "pm", false, 1);
        assert!(choose_planner(&[disabled], None).is_none());
    }

    #[test]
    fn oldest_enabled_pm_wins_when_the_assignee_is_not_a_pm() {
        let newer = agent(4, "PM", "pm", true, 50);
        let older = agent(5, "Product Manager", "pm_two", true, 3);
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        let agents = [newer.clone(), engineer.clone(), older.clone()];
        let chosen = choose_planner(&agents, Some(engineer.id));
        assert_eq!(chosen.map(|agent| agent.id), Some(older.id));
    }
}
