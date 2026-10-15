//! Planning runs and the Plan Review gate.
//!
//! The PM writes the plan while the ticket is in Backlog. When that plan is
//! posted, the ticket moves to Plan Review. Approving it moves the ticket to
//! Ready and starts the Tech Lead when one is available. Skip planning sends
//! the ticket straight from Backlog to Ready. A missing Tech Lead does not
//! block the ticket: Ready stays put and a comment explains why.

use crate::copy::plan::{
    self, format_plan, APPROVE_ONLY_FROM_PLAN_REVIEW, ASK_COMMENT_REQUIRED,
    ASK_ONLY_FROM_PLAN_REVIEW, IMPLEMENTATION_NOT_STARTED, NO_PLAN_YET, NO_PM, NO_TECH_LEAD,
    PLAN_ALREADY_RUNNING, PLAN_APPROVED_NOTE, PLAN_STALE, PLAN_START_WRONG_COLUMN,
    WORK_NOT_STARTED,
};
use crate::domain::agent::Agent;
use crate::domain::comment::{AuthorType, Comment, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::substatus::TicketStatus;
use crate::domain::workflow::{is_pm_identity, is_tech_lead_identity, JOB_TYPE_PLAN_TICKET};
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketService};
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

    /// Start the PM's planning run. Used from Backlog, and from Ready when a
    /// ticket is sitting there with no plan (older rows).
    pub async fn start_planning(&self, ticket_id: Uuid) -> Result<StartOutcome, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if !matches!(
            ticket.ticket.status,
            TicketStatus::Backlog | TicketStatus::Ready
        ) {
            return Err(PlanError::Validation(PLAN_START_WRONG_COLUMN.to_string()));
        }
        self.start(ticket_id, None).await
    }

    /// Ready was just entered with an approved plan or Skip planning.
    /// Start the Tech Lead when one exists. A missing Tech Lead leaves the
    /// ticket in Ready and posts an explanation.
    pub async fn on_entered_ready(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::Ready {
            return Ok(());
        }
        let plan = TicketService::new(self.pool)
            .plan_entry(&ticket.ticket)
            .await?;
        if !plan.allows_ready() {
            return Ok(());
        }
        self.start_tech_lead(ticket_id).await
    }

    async fn start_tech_lead(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        let Some(lead) = tech_lead_for_ready(&agents) else {
            self.note(ticket_id, NO_TECH_LEAD).await?;
            return Ok(());
        };
        TicketService::new(self.pool)
            .assign_agent(ticket_id, Some(lead.id))
            .await?;
        let missing = if TicketService::new(self.pool)
            .get(ticket_id)
            .await?
            .ticket
            .approved_plan_comment_id
            .is_some()
        {
            IMPLEMENTATION_NOT_STARTED
        } else {
            WORK_NOT_STARTED
        };
        match RunService::new(self.pool)
            .start_run_for_agent(
                ticket_id,
                lead.id,
                "work_on_ticket",
                crate::services::run_service::StartRunOptions::default(),
            )
            .await
        {
            Ok(_) | Err(RunError::ActiveRunExists) => Ok(()),
            Err(RunError::Validation(_)) => self.note(ticket_id, missing).await,
            Err(RunError::Database(err)) => Err(PlanError::Database(err)),
            Err(RunError::NotFound) => Ok(()),
        }
    }

    async fn note(&self, ticket_id: Uuid, body: &str) -> Result<(), PlanError> {
        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::System,
                None,
                body,
                CommentIntent::SystemEvent,
                &[],
                &[],
            )
            .await?;
        Ok(())
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

        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        let Some(planner) = planner_for_ticket(&agents) else {
            self.note(ticket_id, NO_PM).await?;
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
        CommentService::new(self.pool)
            .get(comment.id)
            .await
            .map_err(Into::into)
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
        sqlx::query("UPDATE ticket_comments SET plan_content_version = $2 WHERE id = $1")
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

        if let Some(next) = status_after_planning_run(ticket.ticket.status) {
            TicketService::new(self.pool)
                .update_status(ticket_id, next, None, None)
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
            .update_status(ticket_id, TicketStatus::Ready, None, None)
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

        self.start_tech_lead(ticket_id).await?;
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

/// Where a finished planning run moves the ticket. The agent's next status
/// is not an input: Backlog and a stranded Ready ticket go to Plan Review.
pub fn status_after_planning_run(current: TicketStatus) -> Option<TicketStatus> {
    match current {
        TicketStatus::Backlog | TicketStatus::Ready => Some(TicketStatus::PlanReview),
        _ => None,
    }
}

/// The enabled PM who writes the plan. The earliest PM wins when several exist.
pub fn planner_for_ticket(agents: &[Agent]) -> Option<&Agent> {
    agents
        .iter()
        .filter(|agent| agent.enabled && is_pm_agent(agent))
        .min_by_key(|agent| agent.created_at)
}

fn is_pm_agent(agent: &Agent) -> bool {
    let key = agent.preset_source.as_deref().unwrap_or("");
    is_pm_identity(key, &agent.role)
}

fn tech_lead_for_ready(agents: &[Agent]) -> Option<&Agent> {
    agents
        .iter()
        .filter(|agent| agent.enabled && is_tech_lead_agent(agent))
        .min_by_key(|agent| agent.created_at)
}

fn is_tech_lead_agent(agent: &Agent) -> bool {
    let key = agent.preset_source.as_deref().unwrap_or("");
    is_tech_lead_identity(key, &agent.role)
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
    fn planner_is_the_enabled_pm() {
        let pm = agent(1, "PM", "pm", true, 1);
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        let agents = [engineer, pm.clone()];
        let chosen = planner_for_ticket(&agents);
        assert_eq!(chosen.map(|agent| agent.id), Some(pm.id));
    }

    #[test]
    fn planner_is_none_without_a_pm() {
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 10);
        assert!(planner_for_ticket(&[engineer]).is_none());
    }

    #[test]
    fn planner_is_none_when_the_pm_is_disabled() {
        let disabled = agent(1, "PM", "pm", false, 1);
        assert!(planner_for_ticket(&[disabled]).is_none());
    }

    #[test]
    fn planner_picks_the_earliest_enabled_pm() {
        let later = agent(2, "PM", "pm", true, 10);
        let earlier = agent(1, "Product Manager", "product", true, 1);
        let agents = [later, earlier.clone()];
        let chosen = planner_for_ticket(&agents);
        assert_eq!(chosen.map(|agent| agent.id), Some(earlier.id));
    }

    #[test]
    fn planning_run_moves_backlog_to_plan_review_with_no_next_status() {
        assert_eq!(
            status_after_planning_run(TicketStatus::Backlog),
            Some(TicketStatus::PlanReview)
        );
        assert_eq!(
            status_after_planning_run(TicketStatus::Ready),
            Some(TicketStatus::PlanReview)
        );
        assert_eq!(status_after_planning_run(TicketStatus::PlanReview), None);
        assert_eq!(status_after_planning_run(TicketStatus::InProgress), None);
    }

    #[test]
    fn approving_a_plan_targets_ready_not_in_progress() {
        // Approval does not read an agent next status. The status write is Ready.
        assert_eq!(
            status_after_planning_run(TicketStatus::PlanReview),
            None,
            "approval is a separate move from the planning run"
        );
    }
}
