//! Planning runs and the Plan Review gate.
//!
//! The PM writes the plan while the ticket is in Backlog. When that plan is
//! posted, the ticket moves to Plan Review. Approving it moves the ticket to
//! Ready and starts the Tech Lead when one is available. Skip planning sends
//! the ticket straight from Backlog to Ready. A missing Tech Lead does not
//! block the ticket: Ready stays put and a comment explains why.
use crate::domain::Id;

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
use crate::domain::ticket::Ticket;
use crate::domain::workflow::{is_pm_identity, is_tech_lead_identity, JOB_TYPE_PLAN_TICKET};
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::result_contract::upsert_markdown_section;
use crate::services::review_capacity::{ReviewCapacityError, ReviewCapacityService};
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketService};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;
use sqlx::{Postgres, Transaction};
pub use crate::domain::workflow::JOB_TYPE_PLAN_TICKET as PLAN_JOB;

#[derive(Debug)]
pub enum StartOutcome {
    Started(Id),
    AlreadyRunning(Id),
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
    pub plan_id: Id,
    pub markdown: String,
    pub steps: Vec<String>,
}

/// One plan revision stored off the comment thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPlan {
    pub id: Id,
    pub ticket_id: Id,
    pub run_id: Option<Id>,
    pub agent_id: Option<Id>,
    pub revision: i32,
    pub body: String,
    pub content_version: i32,
    pub change_request: Option<String>,
    /// The design set the plan reflects, and the option it was written against.
    pub design_set_id: Option<Id>,
    pub design_option_id: Option<String>,
    /// Live plan-run set id → option id the plan was written around.
    pub design_picks: serde_json::Map<String, serde_json::Value>,
    /// The planner judged the change trivial enough to skip Human Review.
    pub suggest_skip_review: bool,
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
    pub async fn start_planning(&self, ticket_id: Id) -> Result<StartOutcome, PlanError> {
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
    pub async fn on_entered_ready(&self, ticket_id: Id) -> Result<(), PlanError> {
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

    async fn start_tech_lead(&self, ticket_id: Id) -> Result<(), PlanError> {
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

    async fn note(&self, ticket_id: Id, body: &str) -> Result<(), PlanError> {
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
        ticket_id: Id,
        trigger_comment_id: Option<Id>,
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

        let mut tx = self.pool.begin().await?;
        let outcome = self
            .queue_plan_run(&mut tx, ticket_id, planner.id, trigger_comment_id)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    /// Insert the plan run and its job on the caller's transaction.
    async fn queue_plan_run(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ticket_id: Id,
        planner_id: Id,
        trigger_comment_id: Option<Id>,
    ) -> Result<StartOutcome, PlanError> {
        let run_id = Id::new();
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
        .bind(planner_id)
        .bind(JOB_TYPE_PLAN_TICKET)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::Full.as_str())
        .bind(trigger_comment_id)
        .fetch_optional(&mut **tx)
        .await?;

        let Some(row) = inserted else {
            let existing = self.active_plan_run(ticket_id).await?;
            return Ok(StartOutcome::AlreadyRunning(existing.unwrap_or(run_id)));
        };
        let inserted_id: Id = row.get("id");

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Id::new())
        .bind(inserted_id)
        .bind(JOB_TYPE_PLAN_TICKET)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut **tx)
        .await?;
        Ok(StartOutcome::Started(inserted_id))
    }

    /// Insert a plan revision. Does not move the ticket; call [`Self::bind_latest_plan`].
    pub async fn insert_plan(
        &self,
        ticket_id: Id,
        agent_id: Id,
        summary: &str,
        run_id: Option<Id>,
        no_visible_change: bool,
        suggest_skip_review: bool,
    ) -> Result<TicketPlan, PlanError> {
        self.insert_plan_body(
            ticket_id,
            Some(agent_id),
            run_id,
            &format_plan(summary),
            None,
            no_visible_change,
            suggest_skip_review,
        )
        .await
    }

    /// Store the plan and move the ticket into Plan Review in one transaction.
    pub async fn insert_plan_and_bind(
        &self,
        ticket_id: Id,
        agent_id: Id,
        summary: &str,
        run_id: Option<Id>,
        no_visible_change: bool,
        suggest_skip_review: bool,
    ) -> Result<TicketPlan, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let status_before = ticket.ticket.status;
        let mut tx = self.pool.begin().await?;
        let plan = self
            .insert_plan_in_tx(
                &mut tx,
                ticket_id,
                &ticket,
                Some(agent_id),
                run_id,
                &format_plan(summary),
                None,
                no_visible_change,
                suggest_skip_review,
            )
            .await?;
        Self::bind_plan_in_tx(&mut tx, ticket_id, status_before).await?;
        tx.commit().await?;
        Ok(plan)
    }

    /// Markers validated at submit can still go stale before finish; drop those with a log.
    async fn filter_live_marker_ids(
        tx: &mut Transaction<'_, Postgres>,
        ticket_id: Id,
        run_id: Option<Id>,
        markers: Vec<Id>,
    ) -> Result<Vec<Id>, sqlx::Error> {
        let mut live = Vec::with_capacity(markers.len());
        for id in markers {
            let published: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM design_sets
                    WHERE id = $1
                      AND ticket_id = $2
                      AND published_at IS NOT NULL
                      AND retired_at IS NULL
                      AND superseded_by IS NULL
                )
                "#,
            )
            .bind(id)
            .bind(ticket_id)
            .fetch_one(&mut **tx)
            .await?;
            if published {
                live.push(id);
            } else if let Some(run_id) = run_id {
                tracing::warn!(
                    %ticket_id,
                    %run_id,
                    marker_set_id = %id,
                    "ignoring design marker with no live published set at plan finish"
                );
            } else {
                tracing::warn!(
                    %ticket_id,
                    marker_set_id = %id,
                    "ignoring design marker with no live published set at plan finish"
                );
            }
        }
        Ok(live)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_plan_body(
        &self,
        ticket_id: Id,
        agent_id: Option<Id>,
        run_id: Option<Id>,
        body: &str,
        revision: Option<i32>,
        no_visible_change: bool,
        suggest_skip_review: bool,
    ) -> Result<TicketPlan, PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        let mut tx = self.pool.begin().await?;
        let plan = self
            .insert_plan_in_tx(
                &mut tx,
                ticket_id,
                &ticket,
                agent_id,
                run_id,
                body,
                revision,
                no_visible_change,
                suggest_skip_review,
            )
            .await?;
        tx.commit().await?;
        Ok(plan)
    }

    async fn insert_plan_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ticket_id: Id,
        ticket: &crate::services::ticket_service::TicketWithDisplay,
        agent_id: Option<Id>,
        run_id: Option<Id>,
        body: &str,
        revision: Option<i32>,
        no_visible_change: bool,
        suggest_skip_review: bool,
    ) -> Result<TicketPlan, PlanError> {
        sqlx::query("SELECT id FROM tickets WHERE id = $1 FOR UPDATE")
            .bind(ticket_id)
            .execute(&mut **tx)
            .await?;
        let next = match revision {
            Some(forced) => forced,
            None => {
                let next: i32 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(revision), 0) + 1 FROM ticket_plans WHERE ticket_id = $1",
                )
                .bind(ticket_id)
                .fetch_one(&mut **tx)
                .await?;
                next
            }
        };
        // A plan with no visible change drops the old options instead of
        // linking them. The flag is the agent's final word, so a set this
        // same run published is retired too.
        let (design_set_id, design_option_id, design_picks) = if no_visible_change {
            sqlx::query(
                r#"
                UPDATE design_sets
                SET retired_at = now()
                WHERE ticket_id = $1
                  AND published_at IS NOT NULL
                  AND superseded_by IS NULL
                  AND retired_at IS NULL
                  AND run_id IN (
                      SELECT id FROM agent_runs WHERE job_type = 'plan_ticket'
                  )
                "#,
            )
            .bind(ticket_id)
            .execute(&mut **tx)
            .await?;
            (None, None, serde_json::Map::new())
        } else {
            // Technical sets follow plan markers, not kind. A set whose id is
            // absent from this plan is replaced (or retired when no markers).
            let markers = Self::filter_live_marker_ids(
                tx,
                ticket_id,
                run_id,
                crate::services::design_markers::design_marker_ids(body),
            )
            .await?;
            if markers.is_empty() {
                sqlx::query(
                    r#"
                    UPDATE design_sets
                    SET retired_at = now()
                    WHERE ticket_id = $1
                      AND published_at IS NOT NULL
                      AND superseded_by IS NULL
                      AND retired_at IS NULL
                      AND kind IN ('architecture', 'module', 'dependency')
                      AND EXISTS (
                          SELECT 1 FROM agent_runs r
                          WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                      )
                    "#,
                )
                .bind(ticket_id)
                .execute(&mut **tx)
                .await?;
            } else {
                // superseded_by is only a tombstone pointer; markers[0] may be a
                // UI set when the plan keeps a UI marker and drops a technical one.
                sqlx::query(
                    r#"
                    UPDATE design_sets
                    SET superseded_by = $2
                    WHERE ticket_id = $1
                      AND published_at IS NOT NULL
                      AND superseded_by IS NULL
                      AND retired_at IS NULL
                      AND kind IN ('architecture', 'module', 'dependency')
                      AND NOT (id = ANY($3))
                      AND EXISTS (
                          SELECT 1 FROM agent_runs r
                          WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                      )
                    "#,
                )
                .bind(ticket_id)
                .bind(markers[0])
                .bind(&markers)
                .execute(&mut **tx)
                .await?;
            }
            // Every live plan-run set → option (chosen or recommended). Legacy
            // columns keep the newest set for older readers.
            let designs: Vec<(Id, Option<String>)> = sqlx::query_as(
                r#"
                SELECT d.id, COALESCE(d.chosen_option_id, d.recommended_option_id)
                FROM design_sets d
                WHERE d.ticket_id = $1
                  AND d.published_at IS NOT NULL
                  AND d.superseded_by IS NULL
                  AND d.retired_at IS NULL
                  AND EXISTS (
                      SELECT 1 FROM agent_runs r
                      WHERE r.id = d.run_id AND r.job_type = 'plan_ticket'
                  )
                ORDER BY d.created_at DESC, d.id DESC
                "#,
            )
            .bind(ticket_id)
            .fetch_all(&mut **tx)
            .await?;
            let mut picks = serde_json::Map::new();
            for (set_id, option) in &designs {
                if let Some(option_id) = option {
                    picks.insert(set_id.to_string(), serde_json::Value::String(option_id.clone()));
                }
            }
            match designs.first() {
                Some((set, option)) => (Some(*set), option.clone(), picks),
                None => (None, None, picks),
            }
        };
        let picks_json = serde_json::Value::Object(design_picks);
        let id = Id::new();
        let row = sqlx::query(
            r#"
            INSERT INTO ticket_plans (
                id, ticket_id, run_id, agent_id, revision, body, content_version,
                design_set_id, design_option_id, design_picks, suggest_skip_review
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, ticket_id, run_id, agent_id, revision, body, content_version,
                      change_request, design_set_id, design_option_id, design_picks,
                      suggest_skip_review, created_at
            "#,
        )
        .bind(id)
        .bind(ticket_id)
        .bind(run_id)
        .bind(agent_id)
        .bind(next)
        .bind(body)
        .bind(ticket.ticket.content_version)
        .bind(design_set_id)
        .bind(design_option_id)
        .bind(&picks_json)
        .bind(suggest_skip_review)
        .fetch_one(&mut **tx)
        .await?;
        Ok(row_to_plan(&row))
    }

    async fn bind_plan_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        ticket_id: Id,
        status_before: TicketStatus,
    ) -> Result<(), PlanError> {
        use crate::domain::ticket::status_to_str;
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
        .execute(&mut **tx)
        .await?;
        if let Some(next) = status_after_planning_run(status_before) {
            sqlx::query(
                r#"
                UPDATE tickets
                SET status = $2,
                    substatus = NULL,
                    substatus_metadata = NULL,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(ticket_id)
            .bind(status_to_str(next))
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Clear approval and move Backlog/Ready into Plan Review.
    pub async fn bind_latest_plan(&self, ticket_id: Id) -> Result<(), PlanError> {
        let Some(plan) = self.latest_plan(ticket_id).await? else {
            return Err(PlanError::Validation(NO_PLAN_YET.into()));
        };
        self.bind_plan(ticket_id, plan.id).await
    }

    async fn bind_plan(&self, ticket_id: Id, _plan_id: Id) -> Result<(), PlanError> {
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
        ticket_id: Id,
        workflow: &crate::config::WorkflowConfig,
        skip_human_review: Option<bool>,
        accept_recommended: bool,
    ) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        if ticket.ticket.status != TicketStatus::PlanReview {
            return Err(PlanError::Validation(APPROVE_ONLY_FROM_PLAN_REVIEW.into()));
        }
        if self.active_plan_run(ticket_id).await?.is_some() {
            return Err(PlanError::Validation(PLAN_ALREADY_RUNNING.into()));
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

        self.refuse_unready_designs(ticket_id, &plan, accept_recommended)
            .await?;

        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = $2,
                approved_plan_content_version = $3,
                skip_human_review = COALESCE($4, skip_human_review),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(plan.id)
        .bind(ticket.ticket.content_version)
        .bind(skip_human_review)
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
            self.note(ticket_id, &plan::split_approved_note(count))
                .await?;
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
        ticket_id: Id,
        description: &str,
        plan_body: &str,
    ) -> Result<(), PlanError> {
        let section =
            crate::services::design_markers::strip_design_markers(&plan::approved_plan_section_body(
                plan_body,
            ));
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

    /// Write a sub-ticket's plan from its split spec and approve it. Call
    /// after every write that can bump the child's content_version: the plan
    /// and the approval both read the version fresh, so they match.
    pub(crate) async fn approve_child_plan(
        &self,
        parent: &crate::domain::ticket::Ticket,
        child_id: Id,
        spec: &crate::domain::workflow::SplitTicketSpec,
        recommended_by: Id,
    ) -> Result<(), PlanError> {
        let body = plan::child_plan(
            &parent.title,
            &spec.description,
            &spec.steps,
            spec.acceptance_criteria.as_deref(),
        );
        let child_plan = self
            .insert_plan_body(
                child_id,
                Some(recommended_by),
                None,
                &body,
                Some(1),
                false,
                false,
            )
            .await?;
        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = $2, approved_plan_content_version = $3, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(child_id)
        .bind(child_plan.id)
        .bind(child_plan.content_version)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Create the sub-tickets an approved plan proposed, each in Backlog with
    /// its own approved plan, and record the parent as split.
    async fn create_planned_children(
        &self,
        parent: &crate::domain::ticket::Ticket,
        pending: &crate::domain::workflow::PendingSplitRecommendation,
        plan_id: Id,
        workflow: &crate::config::WorkflowConfig,
    ) -> Result<usize, PlanError> {
        let children = crate::services::split_service::SplitService::new(self.pool, workflow)
            .create_child_tickets(parent, &pending.splits, pending.recommended_by_agent_id)
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        TicketService::new(self.pool)
            .clear_pending_split_recommendation(parent.id)
            .await?;
        let child_ids: Vec<Id> = children.iter().map(|child| child.id).collect();
        sqlx::query(
            r#"
            INSERT INTO ticket_plan_splits (parent_ticket_id, plan_id, child_order)
            VALUES ($1, $2, $3)
            ON CONFLICT (parent_ticket_id) DO UPDATE
                SET plan_id = EXCLUDED.plan_id,
                    child_order = EXCLUDED.child_order
            "#,
        )
        .bind(parent.id)
        .bind(plan_id)
        .bind(&child_ids)
        .execute(self.pool)
        .await?;
        crate::services::split_autopilot::SplitAutopilot::new(self.pool)
            .on_split_approved(parent.id)
            .await?;
        Ok(children.len())
    }

    /// True when the split parent can be verified: every sub-ticket that is
    /// not archived is Done and merged, nothing is verifying it yet, and it
    /// is not already waiting for Human Review or Done.
    pub async fn verify_available(&self, ticket_id: Id) -> Result<bool, PlanError> {
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
        ticket_id: Id,
        agent_id: Option<Id>,
    ) -> Result<Id, PlanError> {
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
            None => {
                planner_for_ticket(&agents).ok_or_else(|| PlanError::Validation(NO_PM.into()))?
            }
        };
        let run_id = Id::new();
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
        .bind(Id::new())
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
    pub async fn verification_brief(
        &self,
        ticket_id: Id,
    ) -> Result<VerificationBrief, PlanError> {
        let criteria = match self.approved_plan(ticket_id).await? {
            Some(plan) => plan::plan_criteria(&plan.markdown),
            None => {
                let ticket = TicketService::new(self.pool).get(ticket_id).await?;
                crate::services::result_contract::criteria_from_description(
                    &ticket.ticket.description,
                )
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
    pub async fn is_split_parent(&self, ticket_id: Id) -> Result<bool, PlanError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ticket_plan_splits WHERE parent_ticket_id = $1)",
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await?)
    }

    /// The ticket's question round, if the planner asked one.
    pub async fn questions(
        &self,
        ticket_id: Id,
    ) -> Result<Option<PlanQuestionsState>, PlanError> {
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
    pub async fn can_ask_questions(&self, ticket_id: Id) -> Result<bool, PlanError> {
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
        ticket_id: Id,
        run_id: Option<Id>,
        comment_id: Option<Id>,
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
            return Err(PlanError::Validation(
                plan::PLAN_QUESTIONS_ALREADY_ASKED.into(),
            ));
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
    pub async fn send_answers(&self, ticket_id: Id, answers: &[String]) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;
        TicketService::ensure_not_archived(&ticket.ticket)?;
        let Some(state) = self
            .questions(ticket_id)
            .await?
            .filter(|state| state.waiting)
        else {
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

    pub async fn ask_for_changes(
        &self,
        ticket_id: Id,
        body: &str,
    ) -> Result<StartOutcome, PlanError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self.ask_for_changes_in(&mut tx, ticket_id, body).await?;
        tx.commit().await?;
        if matches!(outcome, StartOutcome::NoPlanner) {
            self.note(ticket_id, NO_PM).await?;
        }
        Ok(outcome)
    }

    /// Record the change request, drop the approval and any split proposed with
    /// the plan, and queue the rewrite, all on the caller's transaction. The
    /// caller reads the outcome and decides whether to commit. A missing planner
    /// posts no note here.
    pub async fn ask_for_changes_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ticket_id: Id,
        body: &str,
    ) -> Result<StartOutcome, PlanError> {
        // Two requests take turns, so the second sees the first one's run.
        sqlx::query("SELECT id FROM tickets WHERE id = $1 FOR UPDATE")
            .bind(ticket_id)
            .execute(&mut **tx)
            .await?;
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
            .execute(&mut **tx)
            .await?;
        // A split proposed with the plan goes with it; the revision may propose another.
        let split_from_plan = ticket
            .ticket
            .pending_split_recommendation
            .as_ref()
            .and_then(|pending| pending.get("fromPlan"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        sqlx::query(
            r#"
            UPDATE tickets
            SET approved_plan_id = NULL,
                approved_plan_content_version = NULL,
                pending_split_recommendation =
                    CASE WHEN $2 THEN NULL ELSE pending_split_recommendation END,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(split_from_plan)
        .execute(&mut **tx)
        .await?;

        if ticket.ticket.skip_planning {
            return Ok(StartOutcome::Skipped);
        }
        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        let Some(planner) = planner_for_ticket(&agents) else {
            return Ok(StartOutcome::NoPlanner);
        };
        self.queue_plan_run(tx, ticket_id, planner.id, None).await
    }

    pub async fn approved_plan(&self, ticket_id: Id) -> Result<Option<ApprovedPlan>, PlanError> {
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

    pub async fn active_plan_run(&self, ticket_id: Id) -> Result<Option<Id>, PlanError> {
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
        ticket_id: Id,
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
    pub async fn change_requests(&self, ticket_id: Id) -> Result<Vec<String>, PlanError> {
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

    pub async fn latest_plan(&self, ticket_id: Id) -> Result<Option<TicketPlan>, PlanError> {
        let row = sqlx::query(
            r#"
            SELECT id, ticket_id, run_id, agent_id, revision, body, content_version,
                   change_request, design_set_id, design_option_id, design_picks,
                   suggest_skip_review, created_at
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

    async fn plan_by_id(&self, plan_id: Id) -> Result<Option<TicketPlan>, PlanError> {
        let row = sqlx::query(
            r#"
            SELECT id, ticket_id, run_id, agent_id, revision, body, content_version,
                   change_request, design_set_id, design_option_id, design_picks,
                   suggest_skip_review, created_at
            FROM ticket_plans
            WHERE id = $1
            "#,
        )
        .bind(plan_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_plan))
    }

    /// Live plan-run sets must be picked or accepted at the recommended option,
    /// and none may be out of date relative to the plan.
    async fn refuse_unready_designs(
        &self,
        ticket_id: Id,
        plan: &TicketPlan,
        accept_recommended: bool,
    ) -> Result<(), PlanError> {
        let sets = crate::services::design_service::DesignService::new(
            self.pool,
            std::env::temp_dir(), // dir unused for list queries
        )
        .list_for_ticket(ticket_id)
        .await
        .map_err(|err| PlanError::Validation(err.to_string()))?;
        let live: Vec<_> = sets
            .into_iter()
            .filter(|set| set.from_plan_run && !set.superseded && !set.inherited)
            .collect();
        if live.is_empty() {
            return Ok(());
        }

        let mut out_of_date = false;
        let mut unpicked_sections = Vec::new();
        for set in &live {
            // Same rule as choice_state: both sides must name an option.
            if let (Some(selected), Some(plan_option)) =
                (&set.selected, plan_option_for_set(plan, set.id))
            {
                if selected.id != plan_option {
                    out_of_date = true;
                }
            }
            if set.chosen.is_none() {
                let section = crate::services::design_markers::section_for_set(&plan.body, set.id)
                    .unwrap_or_else(|| set.title.clone());
                unpicked_sections.push(section);
            }
        }
        if out_of_date {
            return Err(PlanError::Validation(
                "The plan wasn't updated to the chosen design option.".into(),
            ));
        }
        if !unpicked_sections.is_empty() && !accept_recommended {
            return Err(PlanError::Validation(format!(
                "Pick a design option or approve with the recommended default for: {}.",
                unpicked_sections.join(", ")
            )));
        }
        Ok(())
    }
}

/// Option id the plan was written around for `set_id` (picks map, else legacy columns).
pub fn plan_option_for_set(plan: &TicketPlan, set_id: Id) -> Option<String> {
    if let Some(value) = plan.design_picks.get(&set_id.to_string()) {
        return value.as_str().map(str::to_string);
    }
    if plan.design_set_id == Some(set_id) {
        return plan.design_option_id.clone();
    }
    None
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
    let picks_value: serde_json::Value = row
        .try_get("design_picks")
        .unwrap_or_else(|_| serde_json::json!({}));
    let design_picks = picks_value.as_object().cloned().unwrap_or_default();
    TicketPlan {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        run_id: row.get("run_id"),
        agent_id: row.get("agent_id"),
        revision: row.get("revision"),
        body: row.get("body"),
        content_version: row.get("content_version"),
        change_request: row.get("change_request"),
        design_set_id: row.get("design_set_id"),
        design_option_id: row.get("design_option_id"),
        design_picks,
        suggest_skip_review: row.get("suggest_skip_review"),
        created_at: row.get("created_at"),
    }
}

/// Notes are joined by a blank line. A note identical to the last one is not
/// repeated, so Try again after a failed rewrite doesn't list it twice.
fn append_change_request(existing: Option<&str>, added: &str) -> String {
    match existing.map(str::trim).filter(|text| !text.is_empty()) {
        Some(prev) if prev == added || prev.ends_with(&format!("\n\n{added}")) => prev.to_string(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAssignee {
    pub id: Id,
    pub name: String,
}

/// PM offered as the assignee. Only an unassigned, unarchived Backlog ticket
/// that still needs a plan, and only when `planner` is an enabled PM.
pub fn suggested_assignee(ticket: &Ticket, planner: Option<&Agent>) -> Option<SuggestedAssignee> {
    let planner = planner.filter(|agent| agent.enabled && is_pm_agent(agent))?;
    if ticket.assignee_agent_id.is_some()
        || ticket.archived_at.is_some()
        || ticket.status != TicketStatus::Backlog
        || ticket.skip_planning
    {
        return None;
    }
    Some(SuggestedAssignee {
        id: planner.id,
        name: planner.name.clone(),
    })
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

    #[test]
    fn append_change_request_skips_a_note_identical_to_the_last() {
        assert_eq!(append_change_request(None, "Use B."), "Use B.");
        assert_eq!(append_change_request(Some("Use B."), "Use B."), "Use B.");
        assert_eq!(
            append_change_request(Some("Use A.\n\nUse B."), "Use B."),
            "Use A.\n\nUse B."
        );
        // Only the last note counts: going back to an earlier one is a new request.
        assert_eq!(
            append_change_request(Some("Use A.\n\nUse B."), "Use A."),
            "Use A.\n\nUse B.\n\nUse A."
        );
        // A note that merely ends the same way is a different note.
        assert_eq!(
            append_change_request(Some("Do not Use B."), "Use B."),
            "Do not Use B.\n\nUse B."
        );
    }

    fn agent(id: u128, role: &str, key: &str, enabled: bool, created_secs: i64) -> Agent {
        Agent {
            id: Id::from_u128(id),
            name: role.into(),
            role: role.into(),
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "claude-code".into(),
            model_provider: None,
            model: None,
            thinking_level: None,
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

    fn backlog_ticket() -> Ticket {
        let now = OffsetDateTime::from_unix_timestamp(1).unwrap();
        Ticket {
            id: Id::from_u128(9),
            board_id: Id::from_u128(8),
            repo_id: None,
            title: "New ticket".into(),
            description: String::new(),
            attachment_ids: vec![],
            status: TicketStatus::Backlog,
            substatus: None,
            substatus_metadata: None,
            priority: None,
            assignee_agent_id: None,
            owner_user_id: None,
            branch_name: None,
            pending_assign_recommendation: None,
            parent_ticket_id: None,
            pending_split_recommendation: None,
            clarification_round: 0,
            skip_planning: false,
            skip_human_review: false,
            content_version: 1,
            approved_plan_id: None,
            approved_plan_content_version: None,
            created_by: "user".into(),
            created_by_id: None,
            created_at: now,
            updated_at: now,
            archived_at: None,
        }
    }

    #[test]
    fn suggested_assignee_is_the_enabled_pm_on_an_unassigned_backlog_ticket() {
        let pm = agent(1, "PM", "pm", true, 1);
        let suggestion = suggested_assignee(&backlog_ticket(), Some(&pm));
        assert_eq!(
            suggestion,
            Some(SuggestedAssignee {
                id: pm.id,
                name: pm.name,
            })
        );
    }

    #[test]
    fn suggested_assignee_is_none_when_the_ticket_is_assigned() {
        let pm = agent(1, "PM", "pm", true, 1);
        let mut ticket = backlog_ticket();
        ticket.assignee_agent_id = Some(Id::from_u128(2));
        assert!(suggested_assignee(&ticket, Some(&pm)).is_none());
    }

    #[test]
    fn suggested_assignee_is_none_when_the_ticket_is_archived() {
        let pm = agent(1, "PM", "pm", true, 1);
        let mut ticket = backlog_ticket();
        ticket.archived_at = Some(ticket.created_at);
        assert!(suggested_assignee(&ticket, Some(&pm)).is_none());
    }

    #[test]
    fn suggested_assignee_is_none_outside_backlog() {
        let pm = agent(1, "PM", "pm", true, 1);
        let mut ticket = backlog_ticket();
        ticket.status = TicketStatus::Ready;
        assert!(suggested_assignee(&ticket, Some(&pm)).is_none());
    }

    #[test]
    fn suggested_assignee_is_none_when_planning_is_skipped() {
        let pm = agent(1, "PM", "pm", true, 1);
        let mut ticket = backlog_ticket();
        ticket.skip_planning = true;
        assert!(suggested_assignee(&ticket, Some(&pm)).is_none());
    }

    #[test]
    fn suggested_assignee_is_none_without_a_pm() {
        let engineer = agent(2, "Backend Engineer", "backend_engineer", true, 1);
        assert!(suggested_assignee(&backlog_ticket(), None).is_none());
        assert!(suggested_assignee(&backlog_ticket(), Some(&engineer)).is_none());
    }

    #[test]
    fn suggested_assignee_is_none_when_the_pm_is_disabled() {
        let disabled = agent(1, "PM", "pm", false, 1);
        assert!(suggested_assignee(&backlog_ticket(), Some(&disabled)).is_none());
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
        assert_eq!(
            append_change_request(None, "Use Postgres."),
            "Use Postgres."
        );
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
