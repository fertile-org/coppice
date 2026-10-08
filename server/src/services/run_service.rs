use crate::domain::comment::AuthorType;
use crate::domain::connector_check::JOB_TYPE_CONNECTOR_CHECK;
use crate::domain::context_profile::ContextProfile;
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::domain::knowledge_compaction::JOB_TYPE_COMPACT_KNOWLEDGE;
use crate::domain::repo::VerificationStatus;
use crate::domain::run::{run_status_from_str, run_status_to_str, AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::substatus::{validate_status_substatus_combo, TicketStatus};
use crate::domain::ticket::{status_from_str, status_to_str, substatus_from_str};
use crate::mcp::token::TokenService;
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::agent_service::AgentError;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::job_service::{JobError, JobService};
use crate::services::mention_service::MentionError;
use crate::services::notification_service::NotificationService;
use crate::services::repo_service::RepoService;
use crate::services::result_contract::ApplyResult;
use crate::services::ticket_service::{TicketError, TicketService};
use crate::services::workflow_service::WorkflowService;
use sqlx::PgPool;
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

const JOB_TYPE_WORK_ON_TICKET: &str = "work_on_ticket";
const JOB_TYPE_CHAT_TURN: &str = "chat_turn";

pub struct StartRunOptions {
    pub context_profile: ContextProfile,
    pub trigger_comment_id: Option<Uuid>,
}

impl Default for StartRunOptions {
    fn default() -> Self {
        Self {
            context_profile: ContextProfile::Full,
            trigger_comment_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentRunWithConnector {
    pub run: AgentRun,
    pub connector: Option<String>,
}

pub struct RunService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("active run already exists")]
    ActiveRunExists,
    #[error("run not found")]
    NotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<TicketError> for RunError {
    fn from(err: TicketError) -> Self {
        match err {
            TicketError::TicketNotFound => RunError::Validation("ticket not found".into()),
            TicketError::Validation(msg) => RunError::Validation(msg),
            TicketError::Database(e) => RunError::Database(e),
            other => RunError::Validation(other.to_string()),
        }
    }
}

impl From<CommentError> for RunError {
    fn from(err: CommentError) -> Self {
        match err {
            CommentError::TicketNotFound => RunError::Validation("ticket not found".into()),
            CommentError::Validation(msg) => RunError::Validation(msg),
            CommentError::Database(e) => RunError::Database(e),
            other => RunError::Validation(other.to_string()),
        }
    }
}

impl From<AgentError> for RunError {
    fn from(err: AgentError) -> Self {
        match err {
            AgentError::AgentNotFound => RunError::Validation("agent not found".into()),
            AgentError::PresetNotFound => RunError::Validation("preset not found".into()),
            AgentError::Validation(msg) => RunError::Validation(msg),
            AgentError::KnowledgeProvenanceConflict => {
                RunError::Validation("agent has immutable knowledge provenance".into())
            }
            AgentError::Database(e) => RunError::Database(e),
        }
    }
}

impl From<MentionError> for RunError {
    fn from(err: MentionError) -> Self {
        match err {
            MentionError::MentionNotFound => RunError::Validation("mention not found".into()),
            MentionError::Agent(e) => RunError::Validation(e.to_string()),
            MentionError::Database(e) => RunError::Database(e),
        }
    }
}

impl From<crate::services::split_service::SplitError> for RunError {
    fn from(err: crate::services::split_service::SplitError) -> Self {
        match err {
            crate::services::split_service::SplitError::Validation(msg) => {
                RunError::Validation(msg)
            }
            crate::services::split_service::SplitError::Ticket(e) => e.into(),
            crate::services::split_service::SplitError::Agent(e) => e.into(),
        }
    }
}

impl From<JobError> for RunError {
    fn from(err: JobError) -> Self {
        match err {
            JobError::NotFound => RunError::NotFound,
            JobError::Database(e) => RunError::Database(e),
        }
    }
}

impl<'a> RunService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn start_run(&self, ticket_id: Uuid) -> Result<AgentRun, RunError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;

        let agent_id = ticket
            .ticket
            .assignee_agent_id
            .ok_or_else(|| RunError::Validation("ticket has no assignee agent".into()))?;

        let repo_id = ticket
            .ticket
            .repo_id
            .ok_or_else(|| RunError::Validation("ticket has no repo".into()))?;

        let repo = RepoService::new(self.pool)
            .verify(repo_id)
            .await
            .map_err(|e| match e {
                crate::services::repo_service::RepoError::NotFound => {
                    RunError::Validation("repo not found".into())
                }
                other => RunError::Validation(other.to_string()),
            })?;

        if repo.verification_status != VerificationStatus::Ready {
            let detail = repo
                .verification_error
                .unwrap_or_else(|| "repository path is not ready".to_string());
            return Err(RunError::Validation(format!(
                "repository path is not ready: {detail}"
            )));
        }

        let (agent_key, agent_role) = self.agent_key_and_role(agent_id).await?;

        let mut tx = self.pool.begin().await?;
        let run_id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (ticket_id, agent_id)
                WHERE status IN ('queued', 'running') AND ticket_id IS NOT NULL
            DO NOTHING
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(agent_id)
        .bind(JOB_TYPE_WORK_ON_TICKET)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::Full.as_str())
        .bind(None::<Uuid>)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Err(RunError::ActiveRunExists);
        };

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(JOB_TYPE_WORK_ON_TICKET)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;

        Self::apply_queued_run_start_status(
            &mut tx,
            ticket_id,
            &agent_key,
            &agent_role,
            JOB_TYPE_WORK_ON_TICKET,
            ContextProfile::Full,
        )
        .await?;

        tx.commit().await?;

        Ok(row_to_run(&row))
    }

    pub async fn get(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, chat_session_id, chat_message_id, agent_id, job_type, status,
                sandbox_profile_id, worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            FROM agent_runs
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(RunError::NotFound)?;

        Ok(row_to_run(&row))
    }

    pub async fn active_for_ticket_agent(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRun>, RunError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, chat_session_id, chat_message_id, agent_id, job_type, status,
                sandbox_profile_id, worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            FROM agent_runs
            WHERE ticket_id = $1
              AND agent_id = $2
              AND status IN ('queued', 'running')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(ticket_id)
        .bind(agent_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().map(row_to_run))
    }

    pub async fn list_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<AgentRunWithConnector>, RunError> {
        let rows = sqlx::query(
            r#"
            SELECT
                ar.id, ar.ticket_id, ar.agent_id, ar.job_type, ar.status, ar.sandbox_profile_id,
                ar.worktree_path, ar.branch_name, ar.error_message, ar.session_id,
                ar.context_profile, ar.trigger_comment_id,
                ar.started_at, ar.ended_at, ar.created_at,
                a.connector
            FROM agent_runs ar
            JOIN agents a ON a.id = ar.agent_id
            WHERE ar.ticket_id = $1
            ORDER BY ar.created_at DESC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| AgentRunWithConnector {
                run: row_to_run(row),
                connector: row.get("connector"),
            })
            .collect())
    }

    pub async fn stop(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $2, ended_at = now(), submitted_result = NULL
            WHERE id = $1 AND status IN ('queued', 'running')
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Cancelled))
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| RunError::Validation("run is not active".into()))?;

        // Cut off gateway access right away rather than waiting for the worker
        // to notice the cancel; a stopped run's submission is already cleared.
        self.revoke_run_tokens(run_id).await;
        JobService::new(self.pool).cancel_for_run(run_id).await?;

        Ok(row_to_run(&row))
    }

    /// Best effort: the run is already terminal, and the worker's own grant
    /// revokes again when the connector returns.
    async fn revoke_run_tokens(&self, run_id: Uuid) {
        if let Err(err) = TokenService::new(self.pool).revoke_for_run(run_id).await {
            tracing::warn!(%run_id, error = %err, "failed to revoke run tool token");
        }
    }

    pub async fn retry(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let run = self.get(run_id).await?;

        if !matches!(run.status, RunStatus::Failed | RunStatus::Cancelled) {
            return Err(RunError::Validation(
                "run can only be retried from failed or cancelled state".into(),
            ));
        }

        let ticket_id = run
            .ticket_id
            .ok_or_else(|| RunError::Validation("chat turns cannot be retried here".into()))?;
        self.start_run(ticket_id).await
    }

    pub async fn mark_running(&self, run_id: Uuid) -> Result<(), RunError> {
        let result = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $2, started_at = COALESCE(started_at, now())
            WHERE id = $1 AND status = $3
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Running))
        .bind(run_status_to_str(RunStatus::Queued))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RunError::Validation("run is not in queued state".into()));
        }
        Ok(())
    }

    pub async fn finish_with_apply(
        &self,
        run_id: Uuid,
        apply: ApplyResult,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    ) -> Result<AgentRun, RunError> {
        let run = self.get(run_id).await?;
        if run.status != RunStatus::Running {
            return Err(RunError::Validation("run is not running".into()));
        }
        let ticket_id = run
            .ticket_id
            .ok_or_else(|| RunError::Validation("chat turn cannot apply ticket result".into()))?;

        if apply.ticket.status.is_some() || apply.ticket.substatus.is_some() {
            let ticket_svc = TicketService::new(self.pool);
            let current = ticket_svc.get(ticket_id).await?;
            let status = apply.ticket.status.unwrap_or(current.ticket.status);
            ticket_svc
                .update_status(
                    ticket_id,
                    status,
                    Some(apply.ticket.substatus),
                    Some(apply.ticket.substatus_metadata),
                )
                .await?;
        }

        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Agent,
                Some(run.agent_id),
                &apply.comment.body,
                apply.comment.intent,
                &[],
                &apply.comment.mentions,
            )
            .await?;

        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET
                status = $2,
                worktree_path = $3,
                branch_name = $4,
                ended_at = now()
            WHERE id = $1 AND status = $5
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(apply.run_status))
        .bind(worktree_path)
        .bind(branch_name)
        .bind(run_status_to_str(RunStatus::Running))
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| RunError::Validation("run is not running".into()))?;

        Ok(row_to_run(&row))
    }

    pub async fn start_run_for_agent(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
        job_type: &str,
        options: StartRunOptions,
    ) -> Result<AgentRun, RunError> {
        let ticket = TicketService::new(self.pool).get(ticket_id).await?;

        let repo_id = ticket
            .ticket
            .repo_id
            .ok_or_else(|| RunError::Validation("ticket has no repo".into()))?;

        let repo = RepoService::new(self.pool)
            .verify(repo_id)
            .await
            .map_err(|e| match e {
                crate::services::repo_service::RepoError::NotFound => {
                    RunError::Validation("repo not found".into())
                }
                other => RunError::Validation(other.to_string()),
            })?;

        if repo.verification_status != VerificationStatus::Ready {
            let detail = repo
                .verification_error
                .unwrap_or_else(|| "repository path is not ready".to_string());
            return Err(RunError::Validation(format!(
                "repository path is not ready: {detail}"
            )));
        }

        let (agent_key, agent_role) = self.agent_key_and_role(agent_id).await?;

        let mut tx = self.pool.begin().await?;
        let run_id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (ticket_id, agent_id)
                WHERE status IN ('queued', 'running') AND ticket_id IS NOT NULL
            DO NOTHING
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(agent_id)
        .bind(job_type)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(options.context_profile.as_str())
        .bind(options.trigger_comment_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Err(RunError::ActiveRunExists);
        };

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(job_type)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;

        Self::apply_queued_run_start_status(
            &mut tx,
            ticket_id,
            &agent_key,
            &agent_role,
            job_type,
            options.context_profile,
        )
        .await?;

        tx.commit().await?;

        Ok(row_to_run(&row))
    }

    async fn agent_key_and_role(&self, agent_id: Uuid) -> Result<(String, String), RunError> {
        let agent = AgentService::new(self.pool).get(agent_id).await?;
        let agent_key = agent
            .preset_source
            .clone()
            .unwrap_or_else(|| slugify(&agent.name));
        Ok((agent_key, agent.role))
    }

    /// Move Ready/Backlog implementer work to `in_progress` in the same
    /// transaction that inserts the queued run. A later update would let other
    /// connections observe the run while the ticket is still `ready`.
    async fn apply_queued_run_start_status(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        ticket_id: Uuid,
        agent_key: &str,
        agent_role: &str,
        job_type: &str,
        context_profile: ContextProfile,
    ) -> Result<(), RunError> {
        let row = sqlx::query(
            r#"
            SELECT status, substatus, substatus_metadata,
                   skip_planning, content_version,
                   approved_plan_comment_id, approved_plan_content_version
            FROM tickets
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| RunError::Validation("ticket not found".into()))?;

        let status_str: String = row.get("status");
        let current = status_from_str(&status_str).unwrap_or(TicketStatus::Backlog);
        let substatus_str: Option<String> = row.get("substatus");
        let substatus = substatus_str.as_deref().and_then(substatus_from_str);
        let substatus_metadata: Option<serde_json::Value> = row.get("substatus_metadata");
        let Some(new_status) = WorkflowService::resolve_run_start_transition(
            current,
            agent_key,
            agent_role,
            job_type,
            context_profile,
        ) else {
            return Ok(());
        };

        if new_status == TicketStatus::InProgress {
            let skip_planning: bool = row.get("skip_planning");
            let content_version: i32 = row.get("content_version");
            let approved_id: Option<Uuid> = row.get("approved_plan_comment_id");
            let approved_version: Option<i32> = row.get("approved_plan_content_version");
            let approved = if skip_planning {
                false
            } else {
                match (approved_id, approved_version) {
                    (Some(comment_id), Some(version)) if version == content_version => {
                        let latest: Option<(Uuid, Option<i32>)> = sqlx::query_as(
                            r#"
                            SELECT id, plan_content_version
                            FROM ticket_comments
                            WHERE ticket_id = $1 AND intent = 'plan'
                            ORDER BY created_at DESC, id DESC
                            LIMIT 1
                            "#,
                        )
                        .bind(ticket_id)
                        .fetch_optional(&mut **tx)
                        .await?;
                        latest.is_some_and(|(id, plan_version)| {
                            id == comment_id && plan_version == Some(content_version)
                        })
                    }
                    _ => false,
                }
            };
            let plan = crate::services::workflow_service::PlanEntry {
                skip_planning,
                approved_for_current_version: approved,
            };
            if let Some(msg) =
                WorkflowService::direct_status_change_error(current, new_status, plan)
            {
                return Err(RunError::Validation(msg.to_string()));
            }
        }

        if let Some(msg) =
            validate_status_substatus_combo(new_status, substatus, &substatus_metadata)
        {
            return Err(RunError::Validation(msg.to_string()));
        }

        sqlx::query("UPDATE tickets SET status = $2, updated_at = now() WHERE id = $1")
            .bind(ticket_id)
            .bind(status_to_str(new_status))
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn start_chat_turn(
        &self,
        chat_session_id: Uuid,
        chat_message_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRun, RunError> {
        let mut tx = self.pool.begin().await?;
        let run_id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, chat_session_id, chat_message_id,
                agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8, NULL)
            ON CONFLICT (chat_session_id, agent_id)
                WHERE status IN ('queued', 'running') AND chat_session_id IS NOT NULL
            DO NOTHING
            RETURNING
                id, ticket_id, chat_session_id, chat_message_id,
                agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(chat_session_id)
        .bind(chat_message_id)
        .bind(agent_id)
        .bind(JOB_TYPE_CHAT_TURN)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::Conversation.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Err(RunError::ActiveRunExists);
        };

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(JOB_TYPE_CHAT_TURN)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row_to_run(&row))
    }

    /// Queue the agent run for a knowledge compaction batch inside the caller's
    /// transaction so the batch and its run commit together.
    pub async fn start_compaction_run_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        batch_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRun, RunError> {
        let run_id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, chat_session_id, compaction_batch_id,
                agent_id, job_type, status, sandbox_profile_id, context_profile
            )
            VALUES ($1, NULL, NULL, $2, $3, $4, $5, $6, $7)
            RETURNING
                id, ticket_id, chat_session_id, chat_message_id,
                agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(batch_id)
        .bind(agent_id)
        .bind(JOB_TYPE_COMPACT_KNOWLEDGE)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::KnowledgeCompaction.as_str())
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(JOB_TYPE_COMPACT_KNOWLEDGE)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut **tx)
        .await?;

        Ok(row_to_run(&row))
    }

    /// Queue the agent run for a connector check inside the caller's
    /// transaction so the check and its run commit together.
    pub async fn create_connector_check_run(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        check_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRun, RunError> {
        let run_id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, chat_session_id, connector_check_id,
                agent_id, job_type, status, sandbox_profile_id, context_profile
            )
            VALUES ($1, NULL, NULL, $2, $3, $4, $5, $6, $7)
            RETURNING
                id, ticket_id, chat_session_id, chat_message_id,
                agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(check_id)
        .bind(agent_id)
        .bind(JOB_TYPE_CONNECTOR_CHECK)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .bind(ContextProfile::ConnectorCheck.as_str())
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(run_id)
        .bind(JOB_TYPE_CONNECTOR_CHECK)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut **tx)
        .await?;

        Ok(row_to_run(&row))
    }

    pub async fn finish_run(
        &self,
        run_id: Uuid,
        run_status: RunStatus,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    ) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET
                status = $2,
                worktree_path = $3,
                branch_name = $4,
                ended_at = now()
            WHERE id = $1 AND status = $5
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(run_status))
        .bind(worktree_path)
        .bind(branch_name)
        .bind(run_status_to_str(RunStatus::Running))
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| RunError::Validation("run is not running".into()))?;

        Ok(row_to_run(&row))
    }

    pub async fn finish_failed(&self, run_id: Uuid, message: &str) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $2, error_message = $3, ended_at = now(),
                submitted_result = NULL
            WHERE id = $1 AND status IN ($4, $5)
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Failed))
        .bind(message)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(run_status_to_str(RunStatus::Running))
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| RunError::Validation("run is not active".into()))?;

        self.revoke_run_tokens(run_id).await;
        JobService::new(self.pool)
            .fail_active_jobs_for_run(run_id)
            .await?;

        Ok(row_to_run(&row))
    }

    pub async fn set_session_id(&self, run_id: Uuid, session_id: &str) -> Result<(), RunError> {
        sqlx::query("UPDATE agent_runs SET session_id = $2 WHERE id = $1")
            .bind(run_id)
            .bind(session_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_interrupted(&self, run_id: Uuid, reason: &str) -> Result<AgentRun, RunError> {
        let message = format!("interrupted: {reason}");
        let run = self.finish_failed(run_id, &message).await?;

        // Recovery paths can call this service without an API or worker event
        // publisher. Attempt the durable notification at the transition; the
        // source key makes publication retries safe.
        if let Some(ticket_id) = run.ticket_id {
            if let Err(err) = NotificationService::new(self.pool)
                .create_for_run_finished(
                    run.id,
                    ticket_id,
                    run.agent_id,
                    run_status_to_str(run.status),
                )
                .await
            {
                tracing::warn!(error = %err, run_id = %run.id, "failed to create interrupted-run notification");
            }
        }

        Ok(run)
    }

    pub async fn list_active_runs(&self) -> Result<Vec<AgentRun>, RunError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                context_profile, trigger_comment_id,
                started_at, ended_at, created_at
            FROM agent_runs
            WHERE status IN ('queued', 'running')
            ORDER BY created_at
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_run).collect())
    }

    pub async fn agent_connector_for_run(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<String>, RunError> {
        sqlx::query_scalar("SELECT connector FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(self.pool)
            .await
            .map_err(RunError::from)
    }

    pub async fn is_cancelled(&self, run_id: Uuid) -> Result<bool, RunError> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
                .bind(run_id)
                .fetch_optional(self.pool)
                .await?;

        match status {
            Some(s) => Ok(run_status_from_str(&s) == Some(RunStatus::Cancelled)),
            None => Err(RunError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::job_status_to_str;
    use crate::domain::run::RunStatus;

    async fn test_pool() -> Option<PgPool> {
        let pool = crate::db::shared_test_pool().await.ok()?;
        crate::db::truncate_test_workspace(&pool).await.ok()?;
        Some(pool)
    }

    async fn insert_run(pool: &PgPool, status: RunStatus) -> Uuid {
        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("test board")
            .bind(format!("test-{}", board_id))
            .execute(pool)
            .await
            .expect("insert board");

        let agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, responsibilities, system_prompt, connector
            )
            VALUES ($1, $2, $3, '{}', $4, $5)
            "#,
        )
        .bind(agent_id)
        .bind("test agent")
        .bind("worker")
        .bind("prompt")
        .bind("mock")
        .execute(pool)
        .await
        .expect("insert agent");

        let ticket_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(ticket_id)
        .bind(board_id)
        .bind("test ticket")
        .bind("todo")
        .bind("test")
        .bind(agent_id)
        .execute(pool)
        .await
        .expect("insert ticket");

        let run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(agent_id)
        .bind(JOB_TYPE_WORK_ON_TICKET)
        .bind(run_status_to_str(status))
        .bind(PROFILE_ID)
        .execute(pool)
        .await
        .expect("insert run");

        run_id
    }

    #[tokio::test]
    async fn mark_interrupted_sets_failed_status() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = insert_run(&pool, RunStatus::Running).await;
        let svc = RunService::new(&pool);

        let updated = svc
            .mark_interrupted(run_id, "server restarted during run")
            .await
            .expect("mark interrupted");

        assert_eq!(updated.status, RunStatus::Failed);
        assert_eq!(
            updated.error_message.as_deref(),
            Some("interrupted: server restarted during run")
        );
        assert!(updated.ended_at.is_some());
    }

    #[tokio::test]
    async fn mark_interrupted_fails_pending_jobs_for_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = insert_run(&pool, RunStatus::Queued).await;
        let job_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(job_id)
        .bind(run_id)
        .bind(JOB_TYPE_WORK_ON_TICKET)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&pool)
        .await
        .expect("insert job");

        RunService::new(&pool)
            .mark_interrupted(run_id, "server restarted during run")
            .await
            .expect("mark interrupted");

        let status: String = sqlx::query_scalar("SELECT status FROM agent_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("job status");
        assert_eq!(status, "failed");
    }

    #[tokio::test]
    async fn mark_interrupted_does_not_overwrite_cancelled_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = insert_run(&pool, RunStatus::Cancelled).await;
        let svc = RunService::new(&pool);

        let result = svc
            .mark_interrupted(run_id, "late watchdog observation")
            .await;

        assert!(matches!(result, Err(RunError::Validation(_))));
        assert_eq!(
            svc.get(run_id).await.expect("load run").status,
            RunStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn list_active_runs_returns_queued_and_running() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let running_id = insert_run(&pool, RunStatus::Running).await;
        let queued_id = insert_run(&pool, RunStatus::Queued).await;
        let succeeded_id = insert_run(&pool, RunStatus::Succeeded).await;

        let svc = RunService::new(&pool);
        let active = svc.list_active_runs().await.expect("list active runs");
        let active_ids: Vec<Uuid> = active.iter().map(|run| run.id).collect();

        assert!(active_ids.contains(&running_id));
        assert!(active_ids.contains(&queued_id));
        assert!(!active_ids.contains(&succeeded_id));
    }

    #[tokio::test]
    async fn finish_run_does_not_overwrite_cancelled_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = insert_run(&pool, RunStatus::Cancelled).await;
        let svc = RunService::new(&pool);

        let result = svc
            .finish_run(run_id, RunStatus::Succeeded, None, None)
            .await;

        assert!(matches!(result, Err(RunError::Validation(_))));
        assert_eq!(
            svc.get(run_id).await.expect("load run").status,
            RunStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn finish_failed_does_not_overwrite_cancelled_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let run_id = insert_run(&pool, RunStatus::Cancelled).await;
        let svc = RunService::new(&pool);

        let result = svc.finish_failed(run_id, "late failure").await;

        assert!(matches!(result, Err(RunError::Validation(_))));
        assert_eq!(
            svc.get(run_id).await.expect("load run").status,
            RunStatus::Cancelled
        );
    }

    /// The run row and `Ready → in_progress` commit together. A deferred check
    /// sees the ticket at commit time, which is the first moment another
    /// connection can see the run.
    #[tokio::test]
    async fn ready_implementer_run_is_in_progress_when_the_run_commits() {
        let Some(pool) = test_pool().await else {
            return;
        };
        install_ready_run_visibility_check(&pool).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&path)
            .output()
            .expect("git init");
        std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
        std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@localhost")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@localhost")
            .current_dir(&path)
            .output()
            .expect("git commit");

        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("handoff board")
            .bind(format!("handoff-{}", board_id))
            .execute(&pool)
            .await
            .expect("insert board");

        let agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', $4, $5, $6)
            "#,
        )
        .bind(agent_id)
        .bind("Ready Backend Engineer")
        .bind("Backend Engineer")
        .bind("prompt")
        .bind("mock")
        .bind("backend_engineer")
        .execute(&pool)
        .await
        .expect("insert agent");

        let repo_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO repos (id, name, local_path, default_branch, verification_status)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(repo_id)
        .bind("handoff-repo")
        .bind(path.to_string_lossy().as_ref())
        .bind("main")
        .bind("ready")
        .execute(&pool)
        .await
        .expect("insert repo");

        let ticket_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, repo_id, title, status, created_by, assignee_agent_id, skip_planning
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, true)
            "#,
        )
        .bind(ticket_id)
        .bind(board_id)
        .bind(repo_id)
        .bind("handoff ticket")
        .bind("ready")
        .bind("test")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("insert ticket");

        RunService::new(&pool)
            .start_run_for_agent(
                ticket_id,
                agent_id,
                JOB_TYPE_WORK_ON_TICKET,
                StartRunOptions::default(),
            )
            .await
            .expect("queue implementer run");

        let status: String = sqlx::query_scalar("SELECT status FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_one(&pool)
            .await
            .expect("ticket status");
        assert_eq!(status, "in_progress");
    }
}

#[cfg(test)]
async fn install_ready_run_visibility_check(pool: &PgPool) {
    sqlx::query(
        r#"
        CREATE OR REPLACE FUNCTION coppice_test_ready_run_visible()
        RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            IF NEW.job_type = 'work_on_ticket'
               AND NEW.ticket_id IS NOT NULL
               AND EXISTS (
                   SELECT 1
                   FROM tickets
                   JOIN agents ON agents.id = NEW.agent_id
                   WHERE tickets.id = NEW.ticket_id
                     AND tickets.status = 'ready'
                     AND agents.role ILIKE '%engineer%'
                     AND COALESCE(agents.preset_source, '') <> 'tech_lead'
               )
            THEN
                RAISE EXCEPTION 'work run committed while ticket still ready';
            END IF;
            RETURN NEW;
        END;
        $$
        "#,
    )
    .execute(pool)
    .await
    .expect("install ready-run visibility function");

    sqlx::query(
        r#"
        CREATE CONSTRAINT TRIGGER coppice_test_ready_run_visible
        AFTER INSERT ON agent_runs
        DEFERRABLE INITIALLY DEFERRED
        FOR EACH ROW
        EXECUTE FUNCTION coppice_test_ready_run_visible()
        "#,
    )
    .execute(pool)
    .await
    .expect("install ready-run visibility trigger");
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> AgentRun {
    let status_str: String = row.get("status");
    let status = run_status_from_str(&status_str).unwrap_or(RunStatus::Queued);
    let profile_str: String = row.get("context_profile");
    let context_profile = ContextProfile::from_str(&profile_str).unwrap_or(ContextProfile::Full);

    AgentRun {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        chat_session_id: row.try_get("chat_session_id").ok().flatten(),
        chat_message_id: row.try_get("chat_message_id").ok().flatten(),
        agent_id: row.get("agent_id"),
        job_type: row.get("job_type"),
        status,
        sandbox_profile_id: row.get("sandbox_profile_id"),
        worktree_path: row.get("worktree_path"),
        branch_name: row.get("branch_name"),
        error_message: row.get("error_message"),
        session_id: row.get("session_id"),
        context_profile,
        trigger_comment_id: row.get("trigger_comment_id"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        created_at: row.get("created_at"),
    }
}
