use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct KnowledgeJob {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub ticket_id: Option<Uuid>,
    pub revision_id: Option<Uuid>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub locked_by: String,
    pub claim_token: Uuid,
}

pub struct KnowledgeJobService<'a> {
    pool: &'a PgPool,
}

impl<'a> KnowledgeJobService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn claim_next(
        &self,
        worker_id: &str,
        stale_lock_secs: u64,
    ) -> Result<Option<KnowledgeJob>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            UPDATE knowledge_jobs
            SET status = CASE
                    WHEN attempts >= max_attempts THEN 'failed'
                    ELSE 'pending'
                END,
                locked_at = NULL, locked_by = NULL,
                claim_token = NULL,
                available_at = now(), updated_at = now(),
                last_error = COALESCE(
                    last_error,
                    CASE
                        WHEN attempts >= max_attempts
                            THEN 'stale worker lock exhausted maximum attempts'
                        ELSE 'reclaimed stale worker lock'
                    END
                )
            WHERE status = 'running'
              AND locked_at < now() - make_interval(secs => $1::double precision)
            "#,
        )
        .bind(stale_lock_secs as f64)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE knowledge_jobs
            SET status = 'failed', updated_at = now(),
                locked_at = NULL, locked_by = NULL, claim_token = NULL,
                last_error = COALESCE(last_error, 'maximum attempts exhausted before claim')
            WHERE status = 'pending' AND attempts >= max_attempts
            "#,
        )
        .execute(&mut *tx)
        .await?;

        let row = sqlx::query(
            r#"
            SELECT id FROM knowledge_jobs
            WHERE status = 'pending' AND attempts < max_attempts AND available_at <= now()
            ORDER BY available_at, created_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let id: Uuid = row.try_get("id")?;
        let row = sqlx::query(
            r#"
            UPDATE knowledge_jobs
            SET status = 'running', attempts = attempts + 1,
                locked_at = now(), locked_by = $2, claim_token = gen_random_uuid(),
                updated_at = now()
            WHERE id = $1
            RETURNING id, kind, status, ticket_id, revision_id, attempts, max_attempts,
                      locked_by, claim_token
            "#,
        )
        .bind(id)
        .bind(worker_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(row_to_job(&row)?))
    }

    pub async fn mark_completed(&self, job: &KnowledgeJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE knowledge_jobs
            SET status = 'completed', completed_at = now(), updated_at = now(),
                locked_at = NULL, locked_by = NULL, claim_token = NULL, last_error = NULL
            WHERE id = $1 AND status = 'running' AND locked_by = $2 AND claim_token = $3
            "#,
        )
        .bind(job.id)
        .bind(&job.locked_by)
        .bind(job.claim_token)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_error(&self, job: &KnowledgeJob, error: &str) -> Result<(), sqlx::Error> {
        let bounded = error.chars().take(2_000).collect::<String>();
        if job.attempts >= job.max_attempts {
            sqlx::query(
                r#"
                UPDATE knowledge_jobs
                SET status = 'failed', last_error = $4, updated_at = now(),
                    locked_at = NULL, locked_by = NULL, claim_token = NULL
                WHERE id = $1 AND status = 'running' AND locked_by = $2
                  AND claim_token = $3
                "#,
            )
            .bind(job.id)
            .bind(&job.locked_by)
            .bind(job.claim_token)
            .bind(bounded)
            .execute(self.pool)
            .await?;
        } else {
            let exponent = u32::try_from(job.attempts.max(1) - 1).unwrap_or(0).min(8);
            let delay_secs = 1_i64.checked_shl(exponent).unwrap_or(256).min(300);
            let available_at = OffsetDateTime::now_utc() + time::Duration::seconds(delay_secs);
            sqlx::query(
                r#"
                UPDATE knowledge_jobs
                SET status = 'pending', last_error = $4, available_at = $5,
                    updated_at = now(), locked_at = NULL, locked_by = NULL,
                    claim_token = NULL
                WHERE id = $1 AND status = 'running' AND locked_by = $2
                  AND claim_token = $3
                "#,
            )
            .bind(job.id)
            .bind(&job.locked_by)
            .bind(job.claim_token)
            .bind(bounded)
            .bind(available_at)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }
}

fn row_to_job(row: &sqlx::postgres::PgRow) -> Result<KnowledgeJob, sqlx::Error> {
    Ok(KnowledgeJob {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        ticket_id: row.try_get("ticket_id")?,
        revision_id: row.try_get("revision_id")?,
        attempts: row.try_get("attempts")?,
        max_attempts: row.try_get("max_attempts")?,
        locked_by: row.try_get("locked_by")?,
        claim_token: row.try_get("claim_token")?,
    })
}
