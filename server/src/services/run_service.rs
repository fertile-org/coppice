use crate::domain::comment::AuthorType;
use crate::domain::context_profile::ContextProfile;
use crate::domain::repo::VerificationStatus;
use crate::domain::run::{
    run_status_from_str, run_status_to_str, AgentRun, RunStatus,
};
use crate::services::agent_service::AgentService;
use crate::services::repo_service::RepoService;
use crate::sandbox::permissive::PROFILE_ID;
use crate::services::comment_service::{CommentError, CommentService};
use crate::domain::job::{job_status_to_str, JobStatus};
use crate::services::agent_service::AgentError;
use crate::services::job_service::{JobError, JobService};
use crate::services::mention_service::MentionError;
use crate::services::result_contract::ApplyResult;
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::services::workflow_service::WorkflowService;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

const JOB_TYPE_WORK_ON_TICKET: &str = "work_on_ticket";

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
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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

        self.apply_run_start_status(ticket_id, agent_id, JOB_TYPE_WORK_ON_TICKET)
            .await?;

        Ok(row_to_run(&row))
    }

    async fn apply_run_start_status(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
        job_type: &str,
    ) -> Result<Option<TicketWithDisplay>, RunError> {
        let ticket_svc = TicketService::new(self.pool);
        let ticket = ticket_svc.get(ticket_id).await?;
        let agent = AgentService::new(self.pool).get(agent_id).await?;
        let Some(new_status) = WorkflowService::resolve_run_start_transition(
            ticket.ticket.status,
            &agent.role,
            job_type,
        ) else {
            return Ok(None);
        };
        let updated = ticket_svc
            .update_status(ticket_id, new_status, None, None)
            .await?;
        Ok(Some(updated))
    }

    pub async fn get(&self, run_id: Uuid) -> Result<AgentRun, RunError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                worktree_path, branch_name, error_message, session_id,
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
            SET status = $2, ended_at = now()
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

        if apply.ticket.status.is_some() || apply.ticket.substatus.is_some() {
            let ticket_svc = TicketService::new(self.pool);
            let current = ticket_svc.get(run.ticket_id).await?;
            let status = apply
                .ticket
                .status
                .unwrap_or(current.ticket.status);
            ticket_svc
                .update_status(
                    run.ticket_id,
                    status,
                    Some(apply.ticket.substatus),
                    Some(apply.ticket.substatus_metadata),
                )
                .await?;
        }

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
                context_profile, trigger_comment_id,
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
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, trigger_comment_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
        .bind(job_type)
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.apply_run_start_status(ticket_id, agent_id, job_type)
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
            WHERE id = $1
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
                context_profile, trigger_comment_id,
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

    pub async fn mark_interrupted(&self, run_id: Uuid, reason: &str) -> Result<AgentRun, RunError> {
        sqlx::query(
            r#"UPDATE agent_runs SET status = $2, error_message = $3, ended_at = now() WHERE id = $1"#,
        )
        .bind(run_id)
        .bind(run_status_to_str(RunStatus::Failed))
        .bind(format!("interrupted: {reason}"))
        .execute(self.pool)
        .await?;
        self.get(run_id).await
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

    pub async fn agent_connector_for_run(&self, agent_id: Uuid) -> Result<Option<String>, RunError> {
        sqlx::query_scalar("SELECT connector FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(self.pool)
            .await
            .map_err(RunError::from)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::run::RunStatus;

    async fn test_pool() -> Option<PgPool> {
        let pool = crate::db::shared_test_pool().await.ok()?;
        crate::db::truncate_test_workspace(&pool).await.ok()?;
        Some(pool)
    }

    async fn insert_run(pool: &PgPool, status: RunStatus) -> Uuid {
        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind("test project")
            .bind(format!("test-{}", project_id))
            .execute(pool)
            .await
            .expect("insert project");

        let agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5)
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
                id, project_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(ticket_id)
        .bind(project_id)
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
}

fn row_to_run(row: &sqlx::postgres::PgRow) -> AgentRun {
    let status_str: String = row.get("status");
    let status = run_status_from_str(&status_str).unwrap_or(RunStatus::Queued);
    let profile_str: String = row.get("context_profile");
    let context_profile =
        ContextProfile::from_str(&profile_str).unwrap_or(ContextProfile::Full);

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
        context_profile,
        trigger_comment_id: row.get("trigger_comment_id"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        created_at: row.get("created_at"),
    }
}
