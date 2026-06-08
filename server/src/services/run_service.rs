use crate::domain::comment::AuthorType;
use crate::domain::repo::VerificationStatus;
use crate::domain::run::{
    run_status_from_str, run_status_to_str, AgentRun, RunStatus,
};
use crate::services::repo_service::RepoService;
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::comment_service::{CommentError, CommentService};
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::services::job_service::{JobError, JobService};
use crate::services::result_contract::ApplyResult;
use crate::services::ticket_service::{TicketError, TicketService};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

const JOB_TYPE_WORK_ON_TICKET: &str = "work_on_ticket";

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

        let active = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM agent_runs
                WHERE ticket_id = $1 AND agent_id = $2
                  AND status IN ('queued', 'running')
            )
            "#,
        )
        .bind(ticket_id)
        .bind(agent_id)
        .fetch_one(self.pool)
        .await?;

        if active {
            return Err(RunError::ActiveRunExists);
        }

        let mut tx = self.pool.begin().await?;
        let run_id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(agent_id)
        .bind(JOB_TYPE_WORK_ON_TICKET)
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .fetch_one(&mut *tx)
        .await?;

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

        tx.commit().await?;

        Ok(row_to_run(&row))
    }

    pub async fn get(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
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

    pub async fn list_for_ticket(&self, ticket_id: Uuid) -> Result<Vec<AgentRun>, RunError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                started_at, ended_at, created_at
            FROM agent_runs
            WHERE ticket_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_run).collect())
    }

    pub async fn stop(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $2, ended_at = now()
            WHERE id = $1 AND status IN ('queued', 'running')
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Cancelled))
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| RunError::Validation("run is not active".into()))?;

        JobService::new(self.pool)
            .cancel_for_run(run_id)
            .await?;

        Ok(row_to_run(&row))
    }

    pub async fn retry(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let run = self.get(run_id).await?;

        if !matches!(run.status, RunStatus::Failed | RunStatus::Cancelled) {
            return Err(RunError::Validation(
                "run can only be retried from failed or cancelled state".into(),
            ));
        }

        self.start_run(run.ticket_id).await
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
            return Err(RunError::Validation(
                "run is not in queued state".into(),
            ));
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

        TicketService::new(self.pool)
            .update_status(
                run.ticket_id,
                apply.ticket.status,
                Some(apply.ticket.substatus),
                Some(apply.ticket.substatus_metadata),
            )
            .await?;

        CommentService::new(self.pool)
            .create(
                run.ticket_id,
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
            WHERE id = $1
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(apply.run_status))
        .bind(worktree_path)
        .bind(branch_name)
        .fetch_optional(self.pool)
        .await?
        .ok_or(RunError::NotFound)?;

        Ok(row_to_run(&row))
    }

    pub async fn finish_failed(&self, run_id: Uuid, message: &str) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_runs
            SET status = $2, error_message = $3, ended_at = now()
            WHERE id = $1
            RETURNING
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
                started_at, ended_at, created_at
            "#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Failed))
        .bind(message)
        .fetch_optional(self.pool)
        .await?
        .ok_or(RunError::NotFound)?;

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

    pub async fn is_cancelled(&self, run_id: Uuid) -> Result<bool, RunError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM agent_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(self.pool)
        .await?;

        match status {
            Some(s) => Ok(run_status_from_str(&s) == Some(RunStatus::Cancelled)),
            None => Err(RunError::NotFound),
        }
    }
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> AgentRun {
    let status_str: String = row.get("status");
    let status = run_status_from_str(&status_str).unwrap_or(RunStatus::Queued);

    AgentRun {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        agent_id: row.get("agent_id"),
        job_type: row.get("job_type"),
        status,
        sandbox_profile_id: row.get("sandbox_profile_id"),
        worktree_path: row.get("worktree_path"),
        branch_name: row.get("branch_name"),
        error_message: row.get("error_message"),
        session_id: row.get("session_id"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        created_at: row.get("created_at"),
    }
}
