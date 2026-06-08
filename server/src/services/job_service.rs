use crate::domain::job::{
    job_status_from_str, job_status_to_str, AgentJob, JobStatus,
};
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
        let row = sqlx::query(
            r#"
            UPDATE agent_jobs
            SET status = 'processing', locked_at = now(), locked_by = $1, attempts = attempts + 1
            WHERE id = (
                SELECT id FROM agent_jobs
                WHERE status = 'pending' AND available_at <= now()
                ORDER BY available_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING
                id, run_id, job_type, status, attempts, max_attempts,
                available_at, locked_at, locked_by, created_at
            "#,
        )
        .bind(worker_id)
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
            return Err(JobError::NotFound);
        }
        Ok(())
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
