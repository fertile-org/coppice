use crate::domain::job::{job_status_from_str, job_status_to_str, AgentJob, JobStatus};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct JobService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("job not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> JobService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(&self, run_id: Uuid, job_type: &str) -> Result<AgentJob, JobError> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO agent_jobs (id, run_id, job_type, status)
            VALUES ($1, $2, $3, $4)
            RETURNING
                id, run_id, job_type, status, attempts, max_attempts,
                available_at, locked_at, locked_by, created_at
            "#,
        )
        .bind(id)
        .bind(run_id)
        .bind(job_type)
        .bind(job_status_to_str(JobStatus::Pending))
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_job(&row))
    }

    pub async fn claim_next(&self, worker_id: &str) -> Result<Option<AgentJob>, JobError> {
        self.claim_next_excluding(worker_id, &[]).await
    }

    pub async fn claim_next_excluding(
        &self,
        worker_id: &str,
        excluded_connectors: &[String],
    ) -> Result<Option<AgentJob>, JobError> {
        let row = sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = 'processing', locked_at = now(), locked_by = $1, attempts = attempts + 1
            WHERE id = (
                SELECT j.id
                FROM agent_jobs j
                INNER JOIN agent_runs r ON r.id = j.run_id
                LEFT JOIN agents a ON a.id = r.agent_id
                WHERE j.status = 'pending'
                  AND j.available_at <= now()
                  AND (a.connector IS NULL OR NOT (a.connector = ANY($2)))
                ORDER BY j.available_at ASC
                FOR UPDATE OF j SKIP LOCKED
                LIMIT 1
            )
            RETURNING
                id, run_id, job_type, status, attempts, max_attempts,
                available_at, locked_at, locked_by, created_at
            "#,
        )
        .bind(worker_id)
        .bind(excluded_connectors)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(|r| row_to_job(&r)))
    }

    pub async fn mark_done(&self, job_id: Uuid) -> Result<(), JobError> {
        let result = sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(job_status_to_str(JobStatus::Done))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(JobError::NotFound);
        }
        Ok(())
    }

    pub async fn mark_cancelled(&self, job_id: Uuid) -> Result<(), JobError> {
        let result = sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(job_status_to_str(JobStatus::Cancelled))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(JobError::NotFound);
        }
        Ok(())
    }

    pub async fn mark_failed(&self, job_id: Uuid, _message: &str) -> Result<(), JobError> {
        let result = sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(job_status_to_str(JobStatus::Failed))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_jobs WHERE id = $1)")
                    .bind(job_id)
                    .fetch_one(self.pool)
                    .await?;
            if !exists {
                return Err(JobError::NotFound);
            }
        }
        Ok(())
    }

    /// Fail pending and in-flight jobs for a run whose terminal state was set elsewhere
    /// (e.g. server restart sweep, watchdog interrupt).
    pub async fn fail_active_jobs_for_run(&self, run_id: Uuid) -> Result<(), JobError> {
        sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = $2
            WHERE run_id = $1 AND status IN ('pending', 'processing')
            "#,
        )
        .bind(run_id)
        .bind(job_status_to_str(JobStatus::Failed))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_all(&self) -> Result<Vec<AgentJob>, JobError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, run_id, job_type, status, attempts, max_attempts,
                available_at, locked_at, locked_by, created_at
            FROM agent_jobs
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_job).collect())
    }

    pub async fn cancel_for_run(&self, run_id: Uuid) -> Result<(), JobError> {
        sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = $2
            WHERE run_id = $1 AND status = $3
            "#,
        )
        .bind(run_id)
        .bind(job_status_to_str(JobStatus::Cancelled))
        .bind(job_status_to_str(JobStatus::Pending))
        .execute(self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::job_status_to_str;
    use crate::sandbox::permissive::PROFILE_ID;
    use time::{Duration, OffsetDateTime};

    async fn test_pool() -> Option<PgPool> {
        let pool = crate::db::shared_test_pool().await.ok()?;
        crate::db::truncate_test_workspace(&pool).await.ok()?;
        Some(pool)
    }

    async fn insert_agent(pool: &PgPool, connector: &str) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, responsibilities, system_prompt, connector
            )
            VALUES ($1, $2, $3, '{}', $4, $5)
            "#,
        )
        .bind(id)
        .bind(format!("agent-{connector}"))
        .bind("worker")
        .bind("prompt")
        .bind(connector)
        .execute(pool)
        .await
        .expect("insert agent");
        id
    }

    async fn insert_pending_job(
        pool: &PgPool,
        agent_id: Uuid,
        available_at: OffsetDateTime,
    ) -> (Uuid, Uuid) {
        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("claim board")
            .bind(format!("claim-{board_id}"))
            .execute(pool)
            .await
            .expect("insert board");

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
        .bind("claim ticket")
        .bind("backlog")
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
        .bind("work_on_ticket")
        .bind("queued")
        .bind(PROFILE_ID)
        .execute(pool)
        .await
        .expect("insert run");

        let job_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_jobs (
                id, run_id, job_type, status, available_at
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(job_id)
        .bind(run_id)
        .bind("work_on_ticket")
        .bind(job_status_to_str(JobStatus::Pending))
        .bind(available_at)
        .execute(pool)
        .await
        .expect("insert job");

        (run_id, job_id)
    }

    #[tokio::test]
    async fn claim_next_excluding_skips_older_job_on_excluded_connector() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let cursor = insert_agent(&pool, "cursor").await;
        let claude = insert_agent(&pool, "claude-code").await;
        let now = OffsetDateTime::now_utc();
        let (_older_run, older_job) =
            insert_pending_job(&pool, cursor, now - Duration::minutes(2)).await;
        let (_newer_run, newer_job) = insert_pending_job(&pool, claude, now).await;

        let claimed = JobService::new(&pool)
            .claim_next_excluding("worker-test", &["cursor".into()])
            .await
            .expect("claim")
            .expect("a job");
        assert_eq!(claimed.id, newer_job);
        assert_ne!(claimed.id, older_job);
    }

    #[tokio::test]
    async fn claim_next_excluding_still_claims_job_whose_agent_row_is_gone() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let orphan = insert_agent(&pool, "claude-code").await;
        let now = OffsetDateTime::now_utc();
        let (_orphan_run, orphan_job) =
            insert_pending_job(&pool, orphan, now - Duration::minutes(1)).await;

        let mut conn = pool.acquire().await.expect("conn");
        sqlx::query("SET session_replication_role = replica")
            .execute(&mut *conn)
            .await
            .expect("disable FK for orphan agent");
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(orphan)
            .execute(&mut *conn)
            .await
            .expect("delete agent without cascading");
        sqlx::query("SET session_replication_role = DEFAULT")
            .execute(&mut *conn)
            .await
            .expect("restore FK");
        drop(conn);

        let claimed = JobService::new(&pool)
            .claim_next_excluding("worker-test", &["claude-code".into()])
            .await
            .expect("claim")
            .expect("orphan job still claimable");
        assert_eq!(claimed.id, orphan_job);
    }

    #[tokio::test]
    async fn claim_next_empty_exclude_list_claims_oldest() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let cursor = insert_agent(&pool, "cursor").await;
        let claude = insert_agent(&pool, "claude-code").await;
        let now = OffsetDateTime::now_utc();
        let (_older_run, older_job) =
            insert_pending_job(&pool, cursor, now - Duration::minutes(2)).await;
        let (_newer_run, _newer_job) = insert_pending_job(&pool, claude, now).await;

        let claimed = JobService::new(&pool)
            .claim_next("worker-test")
            .await
            .expect("claim")
            .expect("a job");
        assert_eq!(claimed.id, older_job);
    }
}

fn row_to_job(row: &sqlx::postgres::PgRow) -> AgentJob {
    let status_str: String = row.get("status");
    let status = job_status_from_str(&status_str).unwrap_or(JobStatus::Pending);

    AgentJob {
        id: row.get("id"),
        run_id: row.get("run_id"),
        job_type: row.get("job_type"),
        status,
        attempts: row.get("attempts"),
        max_attempts: row.get("max_attempts"),
        available_at: row.get("available_at"),
        locked_at: row.get("locked_at"),
        locked_by: row.get("locked_by"),
        created_at: row.get("created_at"),
    }
}
