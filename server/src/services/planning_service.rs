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
use crate::copy::review::is_ready_hold;
use crate::domain::agent::Agent;
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::substatus::TicketStatus;
use crate::domain::workflow::{is_pm_identity, is_tech_lead_identity, JOB_TYPE_PLAN_TICKET};
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::result_contract::upsert_markdown_section;
use crate::services::review_capacity::{ReviewCapacityError, ReviewCapacityService};
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
    pub plan_id: Uuid,
    pub markdown: String,
    pub steps: Vec<String>,
}

/// One plan revision stored off the comment thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPlan {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub run_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub revision: i32,
    pub body: String,
    pub content_version: i32,
    pub change_request: Option<String>,
    pub created_at: time::OffsetDateTime,
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
        if self.is_split_parent(ticket_id).await? {
            return Ok(());
        }
        if let Some(hold) = ReviewCapacityService::new(self.pool)
            .block_ready_start(ticket_id)
            .await
            .map_err(map_capacity_error)?
        {
            if hold.post {
                self.note(ticket_id, &hold.message).await?;
            }
            return Ok(());
        }
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
            .approved_plan_id
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
            Err(RunError::Validation(message)) if is_ready_hold(&message) => Ok(()),
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

    /// Insert a plan revision. Does not move the ticket; call [`Self::bind_latest_plan`].
    pub async fn insert_plan(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
        summary: &str,
        run_id: Option<Uuid>,
    ) -> Result<TicketPlan, PlanError> {
        self.insert_plan_body(
            ticket_id,
            Some(agent_id),
            run_id,
            &format_plan(summary),
            None,
        )
        .await
    }

    async fn insert_plan_body(
        &self,
        ticket_id: Uuid,
        agent_id: Option<Uuid>,
        run_id: Option<Uuid>,
        body: &str,
        revision: Option<i32>,
    ) -> Result<TicketPlan, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM tickets WHERE id = $1 FOR UPDATE")
            .bind(ticket_id)
            .execute(&mut *tx)
            .await?;
        let next = match revision {
            Some(forced) => forced,
            None => {
                let next: i32 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(revision), 0) + 1 FROM ticket_plans WHERE ticket_id = $1",
                )
                .bind(ticket_id)
                .fetch_one(&mut *tx)
                .await?;
                next
            }
        };
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO ticket_plans (
                id, ticket_id, run_id, agent_id, revision, body, content_version
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, ticket_id, run_id, agent_id, revision, body, content_version,
                      change_request, created_at
            "#,
        )
        .bind(id)
        .bind(ticket_id)
        .bind(run_id)
        .bind(agent_id)
        .bind(next)
        .bind(body)
        .bind(ticket.ticket.content_version)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row_to_plan(&row))
    }

    /// Clear approval and move Backlog/Ready into Plan Review.
    pub async fn bind_latest_plan(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let Some(plan) = self.latest_plan(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        self.bind_plan(ticket_id, plan.id).await
    }

    async fn bind_plan(&self, ticket_id: Uuid, _plan_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = NULL,
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

    pub async fn approve(
        &self,
        ticket_id: Uuid,
        workflow: &crate::config::WorkflowConfig,
    ) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::PlanReview {
            return Err(PlanError::Validation(APPROVE_ONLY_FROM_PLAN_REVIEW.into()));
        }
        if self
            .questions(ticket_id)
            .await?
            .is_some_and(|state| state.waiting)
        {
            return Err(PlanError::Validation(
                plan::APPROVE_WAITING_FOR_ANSWERS.into(),
            ));
        }
        let Some(plan) = self.latest_plan(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        if plan.content_version != ticket.ticket.content_version {
            return Err(PlanError::Validation(PLAN_STALE.into()));
        }

        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = $2,
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

        let plan_split = ticket
            .ticket
            .pending_split_recommendation
            .clone()
            .and_then(|value| {
                serde_json::from_value::<crate::domain::workflow::PendingSplitRecommendation>(value)
                    .ok()
            })
            .filter(|pending| pending.from_plan && !pending.splits.is_empty());
        if let Some(pending) = plan_split {
            let count = self
                .create_planned_children(&ticket.ticket, &pending, plan.id, workflow)
                .await?;
            self.note(ticket_id, &plan::split_approved_note(count)).await?;
            return Ok(());
        }

        self.write_plan_into_description(ticket_id, &ticket.ticket.description, &plan.body)
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

    /// Keep the original description and put the approved plan under its own
    /// section, so the plan is the ticket's refined spec. Not a human edit, so
    /// the content version (and the approval) stays as it is.
    async fn write_plan_into_description(
        &self,
        ticket_id: Uuid,
        description: &str,
        plan_body: &str,
    ) -> Result<(), PlanError> {
        let section = plan::approved_plan_section_body(plan_body);
        if section.is_empty() {
            return Ok(());
        }
        let updated = upsert_markdown_section(description, plan::APPROVED_PLAN_SECTION, &section);
        TicketService::new(self.pool)
            .update_fields(
                ticket_id,
                None,
                Some(&updated),
                None,
                None,
                None,
                None,
                None,
                false,
            )
            .await?;
        Ok(())
    }

    /// Create the sub-tickets an approved plan proposed, each in Backlog with
    /// its own approved plan, and record the parent as split.
    async fn create_planned_children(
        &self,
        parent: &crate::domain::ticket::Ticket,
        pending: &crate::domain::workflow::PendingSplitRecommendation,
        plan_id: Uuid,
        workflow: &crate::config::WorkflowConfig,
    ) -> Result<usize, PlanError> {
        let children = crate::services::split_service::SplitService::new(self.pool, workflow)
            .create_child_tickets(parent, &pending.splits, pending.recommended_by_agent_id)
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        for spec in &pending.splits {
            let Some(child) = children.iter().find(|child| child.title == spec.title) else {
                continue;
            };
            let body = plan::child_plan(
                &parent.title,
                &spec.description,
                &spec.steps,
                spec.acceptance_criteria.as_deref(),
            );
            let child_plan = self
                .insert_plan_body(
                    child.id,
                    Some(pending.recommended_by_agent_id),
                    None,
                    &body,
                    Some(1),
                )
                .await?;
            sqlx::query(
                r#"
                UPDATE tickets
                SET approved_plan_id = $2, approved_plan_content_version = $3, updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(child.id)
            .bind(child_plan.id)
            .bind(child.content_version)
            .execute(self.pool)
            .await?;
        }
        TicketService::new(self.pool)
            .clear_pending_split_recommendation(parent.id)
            .await?;
        sqlx::query(
            r#"
            INSERT INTO ticket_plan_splits (parent_ticket_id, plan_id)
            VALUES ($1, $2)
            ON CONFLICT (parent_ticket_id) DO UPDATE SET plan_id = EXCLUDED.plan_id
            "#,
        )
        .bind(parent.id)
        .bind(plan_id)
        .execute(self.pool)
        .await?;
        Ok(children.len())
    }

    /// True when the split parent can be verified: every sub-ticket that is
    /// not archived is Done and merged, nothing is verifying it yet, and it
    /// is not already waiting for Human Review or Done.
    pub async fn verify_available(&self, ticket_id: Uuid) -> Result<bool, PlanError> {
        let row: (bool, i64, i64, bool) = sqlx::query_as(
            r#"
            SELECT
                EXISTS(SELECT 1 FROM ticket_plan_splits WHERE parent_ticket_id = $1),
                count(child.id) FILTER (WHERE child.archived_at IS NULL),
                count(child.id) FILTER (
                    WHERE child.archived_at IS NULL
                      AND child.status = 'done'
                      AND review.merged_at IS NOT NULL
                ),
                EXISTS(
                    SELECT 1 FROM tickets parent
                    WHERE parent.id = $1
                      AND parent.archived_at IS NULL
                      AND parent.status NOT IN ('wait_for_final_review', 'done')
                )
                AND NOT EXISTS(
                    SELECT 1 FROM agent_runs run
                    WHERE run.ticket_id = $1 AND run.job_type = $2
                      AND run.status IN ('queued', 'running')
                )
            FROM tickets child
            LEFT JOIN ticket_human_reviews review ON review.ticket_id = child.id
            WHERE child.parent_ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(crate::domain::workflow::JOB_TYPE_VERIFY_PARENT)
        .fetch_one(self.pool)
        .await?;
        let (split, live, merged, open) = row;
        Ok(split && open && live > 0 && live == merged)
    }

    /// Start one verification run on a split parent. `agent_id` defaults to
    /// the PM who plans tickets.
    pub async fn start_verification(
        &self,
        ticket_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<Uuid, PlanError> {
        if !self.is_split_parent(ticket_id).await? {
            return Err(PlanError::Validation(plan::VERIFY_NOT_A_PARENT.into()));
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE ticket_id = $1 AND job_type = $2 AND status IN ('queued', 'running'))",
        )
        .bind(ticket_id)
        .bind(crate::domain::workflow::JOB_TYPE_VERIFY_PARENT)
        .fetch_one(self.pool)
        .await?;
        if active {
            return Err(PlanError::Validation(plan::VERIFY_ALREADY_RUNNING.into()));
        }
        if !self.verify_available(ticket_id).await? {
            return Err(PlanError::Validation(plan::VERIFY_NOT_READY.into()));
        }
        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        let agent = match agent_id {
            Some(id) => agents
                .iter()
                .find(|agent| agent.id == id && agent.enabled)
                .ok_or_else(|| PlanError::Validation("Agent not found.".into()))?,
            None => planner_for_ticket(&agents).ok_or_else(|| PlanError::Validation(NO_PM.into()))?,
        };
        let run_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id, context_profile
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(agent.id)
        .bind(crate::domain::workflow::JOB_TYPE_VERIFY_PARENT)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::Full.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO agent_jobs (id, run_id, job_type, status) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(crate::domain::workflow::JOB_TYPE_VERIFY_PARENT)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(run_id)
    }

    /// What a verification run reads: the parent's criteria, and its
    /// sub-tickets as merged or excluded.
    pub async fn verification_brief(&self, ticket_id: Uuid) -> Result<VerificationBrief, PlanError> {
        let criteria = match self.approved_plan(ticket_id).await? {
            Some(plan) => plan::plan_criteria(&plan.markdown),
            None => {
                let ticket = TicketService::new(self.pool).get(ticket_id).await?;
                crate::services::result_contract::criteria_from_description(&ticket.ticket.description)
            }
        };
        let rows: Vec<(String, Option<String>, bool)> = sqlx::query_as(
            r#"
            SELECT child.title, review.merged_sha, child.archived_at IS NOT NULL
            FROM tickets child
            LEFT JOIN ticket_human_reviews review ON review.ticket_id = child.id
            WHERE child.parent_ticket_id = $1
            ORDER BY child.created_at, child.id
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;
        let mut merged = Vec::new();
        let mut excluded = Vec::new();
        for (title, sha, archived) in rows {
            if archived {
                excluded.push(title);
            } else {
                merged.push((title, sha.unwrap_or_default()));
            }
        }
        Ok(VerificationBrief {
            criteria,
            merged,
            excluded,
        })
    }

    /// True for a ticket split at Plan Review. It gets no implementation run.
    pub async fn is_split_parent(&self, ticket_id: Uuid) -> Result<bool, PlanError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ticket_plan_splits WHERE parent_ticket_id = $1)",
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await?)
    }

    /// The ticket's question round, if the planner asked one.
    pub async fn questions(&self, ticket_id: Uuid) -> Result<Option<PlanQuestionsState>, PlanError> {
        let row: Option<(serde_json::Value, Option<serde_json::Value>, bool)> = sqlx::query_as(
            r#"
            SELECT questions, answers, answered_at IS NULL
            FROM ticket_plan_questions
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|(questions, answers, waiting)| PlanQuestionsState {
            questions: serde_json::from_value(questions).unwrap_or_default(),
            answers: answers
                .and_then(|answers| serde_json::from_value(answers).ok())
                .unwrap_or_default(),
            waiting,
        }))
    }

    /// Questions are allowed once, and only before the ticket's first plan.
    pub async fn can_ask_questions(&self, ticket_id: Uuid) -> Result<bool, PlanError> {
        let blocked: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1 FROM ticket_plan_questions WHERE ticket_id = $1)
                OR EXISTS(SELECT 1 FROM ticket_plans WHERE ticket_id = $1)
            "#,
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await?;
        Ok(!blocked)
    }

    /// Store the planner's questions and move the ticket to Plan Review,
    /// where it waits for answers. `comment_id` is the posted questions comment.
    pub async fn record_questions(
        &self,
        ticket_id: Uuid,
        run_id: Option<Uuid>,
        comment_id: Option<Uuid>,
        questions: &[crate::providers::PlanQuestion],
    ) -> Result<(), PlanError> {
        if questions.len() > plan::MAX_PLAN_QUESTIONS {
            return Err(PlanError::Validation(plan::PLAN_QUESTIONS_TOO_MANY.into()));
        }
        let inserted = sqlx::query(
            r#"
            INSERT INTO ticket_plan_questions (ticket_id, run_id, comment_id, questions)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (ticket_id) DO NOTHING
            "#,
        )
        .bind(ticket_id)
        .bind(run_id)
        .bind(comment_id)
        .bind(serde_json::to_value(questions).unwrap_or_default())
        .execute(self.pool)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Err(PlanError::Validation(plan::PLAN_QUESTIONS_ALREADY_ASKED.into()));
        }
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if let Some(next) = status_after_planning_run(ticket.ticket.status) {
            TicketService::new(self.pool)
                .update_status(ticket_id, next, None, None)
                .await?;
        }
        Ok(())
    }

    /// Post the human's answers and start planning again with them. The
    /// ticket stays in Plan Review while the plan is written.
    pub async fn send_answers(&self, ticket_id: Uuid, answers: &[String]) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        TicketService::ensure_not_archived(&ticket.ticket)?;
        let Some(state) = self.questions(ticket_id).await?.filter(|state| state.waiting) else {
            return Err(PlanError::Validation(plan::NO_QUESTIONS_WAITING.into()));
        };
        let answers: Vec<String> = state
            .questions
            .iter()
            .enumerate()
            .map(|(index, _)| {
                answers
                    .get(index)
                    .map(|answer| answer.trim().to_string())
                    .unwrap_or_default()
            })
            .collect();
        if answers.iter().all(String::is_empty) {
            return Err(PlanError::Validation(plan::SEND_ANSWERS_REQUIRED.into()));
        }
        if self.active_plan_run(ticket_id).await?.is_some() {
            return Err(PlanError::Validation(PLAN_ALREADY_RUNNING.into()));
        }
        let body = format!(
            "{}\n\n{}",
            plan::ANSWERS_COMMENT_TITLE,
            plan::answered_questions(&state.questions, &answers)
        );
        let comment = CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Human,
                None,
                body.trim_end(),
                CommentIntent::ClarificationAnswer,
                &[],
                &[],
            )
            .await?;
        sqlx::query(
            r#"
            UPDATE ticket_plan_questions
            SET answers = $2, answers_comment_id = $3, answered_at = now()
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(serde_json::to_value(&answers).unwrap_or_default())
        .bind(comment.id)
        .execute(self.pool)
        .await?;
        self.start(ticket_id, Some(comment.id)).await?;
        Ok(())
    }

    pub async fn ask_for_changes(&self, ticket_id: Uuid, body: &str) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::PlanReview {
            return Err(PlanError::Validation(ASK_ONLY_FROM_PLAN_REVIEW.into()));
        }
        let Some(plan) = self.latest_plan(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        if body.trim().is_empty() {
            return Err(PlanError::Validation(ASK_COMMENT_REQUIRED.into()));
        }
        if self.active_plan_run(ticket_id).await?.is_some() {
            return Err(PlanError::Validation(PLAN_ALREADY_RUNNING.into()));
        }

        let appended = append_change_request(plan.change_request.as_deref(), body.trim());
        sqlx::query("UPDATE ticket_plans SET change_request = $2 WHERE id = $1")
            .bind(plan.id)
            .bind(&appended)
            .execute(self.pool)
            .await?;
        // A split proposed with the plan goes with it; the revision may propose another.
        if ticket
            .ticket
            .pending_split_recommendation
            .as_ref()
            .and_then(|pending| pending.get("fromPlan"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            TicketService::new(self.pool)
                .clear_pending_split_recommendation(ticket_id)
                .await?;
        }

        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = NULL,
                approved_plan_content_version = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .execute(self.pool)
        .await?;

        self.start(ticket_id, None).await?;
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
        let Some(plan_id) = ticket.ticket.approved_plan_id else {
            return Ok(None);
        };
        let Some(plan) = self.plan_by_id(plan_id).await? else {
            return Ok(None);
        };
        Ok(Some(ApprovedPlan {
            plan_id: plan.id,
            steps: plan::checklist_steps(&plan.body),
            markdown: plan.body,
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

    /// For a revision run: the latest plan and every change request, oldest first.
    /// `None` when the latest plan has no change request (a first plan).
    pub async fn revision_context(
        &self,
        ticket_id: Uuid,
    ) -> Result<Option<PlanRevisionContext>, PlanError> {
        let Some(latest) = self.latest_plan(ticket_id).await? else {
            return Ok(None);
        };
        if latest
            .change_request
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            return Ok(None);
        }
        let requests = self.change_requests(ticket_id).await?;
        Ok(Some(PlanRevisionContext {
            previous_plan: latest.body,
            requests,
        }))
    }

    /// Every non-null change request on the ticket, oldest revision first.
    pub async fn change_requests(&self, ticket_id: Uuid) -> Result<Vec<String>, PlanError> {
        let requests: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT change_request
            FROM ticket_plans
            WHERE ticket_id = $1
              AND change_request IS NOT NULL
              AND btrim(change_request) <> ''
            ORDER BY revision ASC, created_at ASC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;
        Ok(requests)
    }

    pub async fn latest_plan(&self, ticket_id: Uuid) -> Result<Option<TicketPlan>, PlanError> {
        let row = sqlx::query(
            r#"
            SELECT id, ticket_id, run_id, agent_id, revision, body, content_version,
                   change_request, created_at
            FROM ticket_plans
            WHERE ticket_id = $1
            ORDER BY revision DESC
            LIMIT 1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_plan))
    }

    async fn plan_by_id(&self, plan_id: Uuid) -> Result<Option<TicketPlan>, PlanError> {
        let row = sqlx::query(
            r#"
            SELECT id, ticket_id, run_id, agent_id, revision, body, content_version,
                   change_request, created_at
            FROM ticket_plans
            WHERE id = $1
            "#,
        )
        .bind(plan_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_plan))
    }
}

/// Input for a verification run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationBrief {
    pub criteria: Vec<String>,
    /// Sub-ticket title and merged commit.
    pub merged: Vec<(String, String)>,
    /// Archived sub-tickets, left out of verification.
    pub excluded: Vec<String>,
}

/// A ticket's question round. `waiting` until the human sends answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanQuestionsState {
    pub questions: Vec<crate::providers::PlanQuestion>,
    pub answers: Vec<String>,
    pub waiting: bool,
}

/// What a revision run is told: the plan it replaces and every request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRevisionContext {
    pub previous_plan: String,
    pub requests: Vec<String>,
}

fn row_to_plan(row: &sqlx::postgres::PgRow) -> TicketPlan {
    TicketPlan {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        run_id: row.get("run_id"),
        agent_id: row.get("agent_id"),
        revision: row.get("revision"),
        body: row.get("body"),
        content_version: row.get("content_version"),
        change_request: row.get("change_request"),
        created_at: row.get("created_at"),
    }
}

fn append_change_request(existing: Option<&str>, added: &str) -> String {
    match existing.map(str::trim).filter(|text| !text.is_empty()) {
        Some(prev) => format!("{prev}\n\n{added}"),
        None => added.to_string(),
    }
}

fn map_capacity_error(err: ReviewCapacityError) -> PlanError {
    match err {
        ReviewCapacityError::Database(err) => PlanError::Database(err),
        ReviewCapacityError::Message(message) => PlanError::Validation(message),
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

    #[test]
    fn a_second_ask_appends_to_the_same_revision() {
        assert_eq!(append_change_request(None, "Use Postgres."), "Use Postgres.");
        assert_eq!(
            append_change_request(Some("Use Postgres."), "Split the API."),
            "Use Postgres.\n\nSplit the API."
        );
        assert_eq!(
            append_change_request(Some("  "), "Only this."),
            "Only this."
        );
    }
}
