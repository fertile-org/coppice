use crate::config::KnowledgeConfig;
use crate::domain::agent::Agent;
use crate::domain::knowledge_compaction::{
    next_scheduled_at, select_within_bytes, BatchStatus, BatchTrigger, CompactionBatch,
};
use crate::knowledge::candidates::{
    review_candidates, BatchTicket, CandidateContext, KnowledgeCandidateSpec,
};
use crate::knowledge::policy::{policy_decision, PolicyInput};
use crate::services::knowledge_service::{CompactionProvenance, KnowledgeError, KnowledgeService};
use crate::services::notification_service::NotificationService;
use crate::services::run_service::{RunError, RunService};
use crate::util::truncate::truncate_with_ellipsis;
use sqlx::Connection;
use std::collections::{HashMap, HashSet};
use crate::services::workspace_settings_service::{
    WorkspaceSettingsError, WorkspaceSettingsService,
};
use sqlx::{PgPool, Row};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const ONE_ACTIVE_BATCH_INDEX: &str = "knowledge_compaction_one_active_batch";
const MAX_SUMMARY_BYTES: usize = 8_000;
const MAX_ERROR_BYTES: usize = 2_000;
pub const NOT_APPLIED_MESSAGE: &str = "compaction run finished but its result was not applied";

const BATCH_SELECT: &str = r#"
SELECT
    b.id, b.agent_id, a.name AS agent_name, r.id AS run_id, b.status, b.trigger,
    b.cycle_id, b.cycle_started_at, b.ticket_count, b.candidate_count, b.summary,
    b.error_message, b.created_by, b.created_at, b.started_at, b.ended_at
FROM knowledge_compaction_batches b
LEFT JOIN agents a ON a.id = b.agent_id
LEFT JOIN agent_runs r ON r.compaction_batch_id = b.id
"#;

#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("no knowledge compaction agent is configured")]
    NotConfigured,
    #[error("the knowledge compaction agent is disabled")]
    AgentDisabled,
    #[error("a knowledge compaction batch is already running")]
    ActiveBatch,
    #[error("no done tickets are waiting for compaction")]
    NothingQueued,
    #[error("there are no failed tickets to retry")]
    NothingToRetry,
    #[error("no knowledge compaction batch is running")]
    NoActiveBatch,
    #[error("knowledge compaction batch {0} is no longer active")]
    BatchNotActive(Uuid),
    #[error(transparent)]
    Knowledge(#[from] KnowledgeError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<WorkspaceSettingsError> for CompactionError {
    fn from(error: WorkspaceSettingsError) -> Self {
        match error {
            WorkspaceSettingsError::Database(error) => Self::Database(error),
            WorkspaceSettingsError::AgentNotFound
            | WorkspaceSettingsError::ConnectorNotReadOnly(_) => Self::NotConfigured,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueCounts {
    /// Unbatched tickets eligible for automatic batching.
    pub queued: i64,
    /// Unbatched tickets that reached `max_attempts`; only a retry picks them up.
    pub blocked: i64,
    pub oldest_queued_at: Option<OffsetDateTime>,
}

/// What a successful batch produced.
#[derive(Debug, Clone, Default)]
pub struct BatchOutcome {
    pub created: usize,
    pub dropped: Vec<String>,
}

/// A batch failure resolved by [`KnowledgeCompactionService::reconcile`].
#[derive(Debug, Clone)]
pub struct ReconciledFailure {
    pub batch_id: Uuid,
    pub notified: bool,
}

pub struct KnowledgeCompactionService<'a> {
    pool: &'a PgPool,
    config: &'a KnowledgeConfig,
}

impl<'a> KnowledgeCompactionService<'a> {
    pub fn new(pool: &'a PgPool, config: &'a KnowledgeConfig) -> Self {
        Self { pool, config }
    }

    /// One scheduler step: continue a successful drain cycle, otherwise start a
    /// scheduled cycle once the interval has elapsed.
    pub async fn tick(&self, now: OffsetDateTime) -> Result<Option<CompactionBatch>, CompactionError> {
        self.reconcile().await?;
        let agent = match self.ready_agent().await {
            Ok(agent) => agent,
            Err(CompactionError::NotConfigured | CompactionError::AgentDisabled) => {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        if self.active_batch().await?.is_some() {
            return Ok(None);
        }

        if let Some(last) = self.last_batch().await? {
            if last.status == BatchStatus::Succeeded {
                let continued = self
                    .create_batch_or_skip(
                        agent.id,
                        last.trigger,
                        last.cycle_id,
                        last.cycle_started_at,
                        last.created_by,
                    )
                    .await?;
                if continued.is_some() {
                    return Ok(continued);
                }
            }
        }

        let counts = self.queue_counts().await?;
        let Some(due_at) = next_scheduled_at(
            self.last_cycle_started_at().await?,
            counts.oldest_queued_at,
            self.interval(),
        ) else {
            return Ok(None);
        };
        if now < due_at {
            return Ok(None);
        }
        self.create_batch_or_skip(agent.id, BatchTrigger::Scheduled, Uuid::new_v4(), now, None)
            .await
    }

    /// "Compact now": start a drain cycle immediately, ignoring the interval.
    pub async fn start_manual(&self, user_id: Uuid) -> Result<CompactionBatch, CompactionError> {
        let agent = self.ready_agent().await?;
        if self.active_batch().await?.is_some() {
            return Err(CompactionError::ActiveBatch);
        }
        self.create_batch(
            agent.id,
            BatchTrigger::Manual,
            Uuid::new_v4(),
            OffsetDateTime::now_utc(),
            Some(user_id),
        )
        .await?
        .ok_or(CompactionError::NothingQueued)
    }

    /// Reset attempts on failed tickets and start a retry cycle.
    pub async fn retry(&self, user_id: Uuid) -> Result<CompactionBatch, CompactionError> {
        let agent = self.ready_agent().await?;
        if self.active_batch().await?.is_some() {
            return Err(CompactionError::ActiveBatch);
        }
        let reset = sqlx::query(
            "UPDATE knowledge_compaction_queue SET attempts = 0 WHERE batch_id IS NULL AND attempts > 0",
        )
        .execute(self.pool)
        .await?
        .rows_affected();
        if reset == 0 {
            return Err(CompactionError::NothingToRetry);
        }
        self.create_batch(
            agent.id,
            BatchTrigger::Retry,
            Uuid::new_v4(),
            OffsetDateTime::now_utc(),
            Some(user_id),
        )
        .await?
        .ok_or(CompactionError::NothingToRetry)
    }

    pub async fn mark_batch_running(&self, batch_id: Uuid) -> Result<(), CompactionError> {
        let updated = sqlx::query(
            r#"
            UPDATE knowledge_compaction_batches
            SET status = 'running', started_at = COALESCE(started_at, now())
            WHERE id = $1 AND status IN ('queued', 'running')
            "#,
        )
        .bind(batch_id)
        .execute(self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(CompactionError::BatchNotActive(batch_id));
        }
        Ok(())
    }

    /// Apply a `done` result: validate candidates, insert the valid ones, mark
    /// the batch succeeded, and drop its queue rows, all in one transaction.
    pub async fn complete_batch(
        &self,
        batch_id: Uuid,
        run_id: Uuid,
        agent_summary: &str,
        specs: &[KnowledgeCandidateSpec],
    ) -> Result<BatchOutcome, CompactionError> {
        let mut tx = self.pool.begin().await?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM knowledge_compaction_batches WHERE id = $1 FOR UPDATE",
        )
        .bind(batch_id)
        .fetch_optional(&mut *tx)
        .await?;
        if !status
            .as_deref()
            .and_then(BatchStatus::parse)
            .is_some_and(BatchStatus::is_active)
        {
            return Err(CompactionError::BatchNotActive(batch_id));
        }

        let tickets = sqlx::query(
            r#"
            SELECT t.id, t.board_id
            FROM knowledge_compaction_batch_tickets bt
            JOIN tickets t ON t.id = bt.ticket_id
            WHERE bt.batch_id = $1
            "#,
        )
        .bind(batch_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|row| {
            Ok(BatchTicket {
                id: row.try_get("id")?,
                board_id: row.try_get("board_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let referenced = |pick: fn(&KnowledgeCandidateSpec) -> Option<&String>| -> Vec<Uuid> {
            specs
                .iter()
                .filter_map(pick)
                .filter_map(|raw| Uuid::parse_str(raw.trim()).ok())
                .collect()
        };
        let agent_ids: HashSet<Uuid> =
            sqlx::query_scalar("SELECT id FROM agents WHERE id = ANY($1)")
                .bind(referenced(|spec| spec.agent_id.as_ref()))
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
        let supersedable: HashMap<Uuid, Option<Uuid>> = sqlx::query(
            r#"
            SELECT i.id, r.board_id
            FROM knowledge_items i
            JOIN knowledge_revisions r ON r.id = i.active_revision_id
            WHERE i.id = ANY($1)
              AND i.status = 'approved'
              AND i.superseded_by IS NULL
              AND (i.expires_at IS NULL OR i.expires_at > now())
            "#,
        )
        .bind(referenced(|spec| spec.supersedes_item_id.as_ref()))
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("board_id")?)))
        .collect::<Result<_, sqlx::Error>>()?;

        let review = review_candidates(
            specs,
            &CandidateContext {
                tickets: &tickets,
                agent_ids: &agent_ids,
                supersedable: &supersedable,
                run_id,
                max_candidates: self.config.compaction.max_candidates_per_batch,
            },
        );
        let mut outcome = BatchOutcome {
            created: 0,
            dropped: review.dropped,
        };
        let knowledge = KnowledgeService::new(self.pool, self.config);
        for candidate in review.accepted {
            let (status, decision, reason) = policy_decision(
                self.config,
                PolicyInput {
                    knowledge_type: candidate.input.knowledge_type,
                    scope: candidate.input.scope,
                    confidence: candidate.input.confidence,
                    requires_human_approval: candidate.requires_human_approval,
                    supersedes: candidate.supersedes_item_id.is_some(),
                },
            );
            let mut savepoint = (*tx).begin().await?;
            let inserted = knowledge
                .create_from_compaction_in_tx(
                    &mut savepoint,
                    CompactionProvenance {
                        batch_id,
                        candidate_index: candidate.index,
                        source_ticket_ids: &candidate.source_ticket_ids,
                        supersedes_item_id: candidate.supersedes_item_id,
                    },
                    candidate.input,
                    status,
                    decision,
                    &reason,
                )
                .await;
            match inserted {
                Ok(_) => {
                    savepoint.commit().await?;
                    outcome.created += 1;
                }
                Err(KnowledgeError::Database(error)) => return Err(error.into()),
                Err(error) => {
                    savepoint.rollback().await?;
                    outcome
                        .dropped
                        .push(format!("candidate {}: {error}", candidate.index + 1));
                }
            }
        }

        let summary = batch_summary(agent_summary, &outcome.dropped);
        sqlx::query(
            r#"
            UPDATE knowledge_compaction_batches
            SET status = 'succeeded', candidate_count = $2, summary = $3,
                error_message = NULL, ended_at = now()
            WHERE id = $1
            "#,
        )
        .bind(batch_id)
        .bind(i32::try_from(outcome.created).unwrap_or(i32::MAX))
        .bind(summary)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM knowledge_compaction_queue WHERE batch_id = $1")
            .bind(batch_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    /// Fail an active batch and release its tickets back to the queue. Returns
    /// false when the batch was already resolved.
    pub async fn fail_batch(
        &self,
        batch_id: Uuid,
        error_message: &str,
        count_attempt: bool,
    ) -> Result<bool, CompactionError> {
        let mut tx = self.pool.begin().await?;
        let failed = sqlx::query(
            r#"
            UPDATE knowledge_compaction_batches
            SET status = 'failed', error_message = $2, ended_at = now()
            WHERE id = $1 AND status IN ('queued', 'running')
            "#,
        )
        .bind(batch_id)
        .bind(truncate_with_ellipsis(error_message.trim(), MAX_ERROR_BYTES))
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if failed {
            sqlx::query(
                r#"
                UPDATE knowledge_compaction_queue
                SET batch_id = NULL, attempts = attempts + CASE WHEN $2 THEN 1 ELSE 0 END
                WHERE batch_id = $1
                "#,
            )
            .bind(batch_id)
            .bind(count_attempt)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(failed)
    }

    /// Resolve active batches whose run already ended without applying a
    /// result. Failed, blocked, and unapplied runs count an attempt and notify
    /// every user; cancelled runs do neither.
    pub async fn reconcile(&self) -> Result<Vec<ReconciledFailure>, CompactionError> {
        let rows = sqlx::query(
            r#"
            SELECT b.id AS batch_id, b.agent_id, r.id AS run_id, r.status, r.error_message
            FROM knowledge_compaction_batches b
            JOIN agent_runs r ON r.compaction_batch_id = b.id
            WHERE b.status IN ('queued', 'running')
              AND r.status IN ('succeeded', 'failed', 'blocked', 'cancelled')
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        let mut resolved = Vec::new();
        for row in rows {
            let batch_id: Uuid = row.try_get("batch_id")?;
            let agent_id: Uuid = row.try_get("agent_id")?;
            let run_id: Uuid = row.try_get("run_id")?;
            let status: String = row.try_get("status")?;
            let run_error: Option<String> = row.try_get("error_message")?;
            let (message, counts_as_failure) = match status.as_str() {
                "cancelled" => ("cancelled".to_string(), false),
                "succeeded" => (NOT_APPLIED_MESSAGE.to_string(), true),
                other => (
                    run_error
                        .filter(|error| !error.trim().is_empty())
                        .unwrap_or_else(|| format!("compaction run {other}")),
                    true,
                ),
            };
            if !self.fail_batch(batch_id, &message, counts_as_failure).await? {
                continue;
            }
            if counts_as_failure {
                NotificationService::new(self.pool)
                    .create_for_knowledge_compaction_failed(batch_id, Some(run_id), agent_id, &message)
                    .await
                    .map_err(|error| match error {
                        crate::services::notification_service::NotificationError::Database(
                            error,
                        ) => CompactionError::Database(error),
                        crate::services::notification_service::NotificationError::NotFound => {
                            CompactionError::Database(sqlx::Error::RowNotFound)
                        }
                    })?;
            }
            resolved.push(ReconciledFailure {
                batch_id,
                notified: counts_as_failure,
            });
        }
        Ok(resolved)
    }

    /// Cancel the active batch's run and release its tickets without counting
    /// an attempt or notifying. Returns the cancelled batch and its run id.
    pub async fn cancel(&self) -> Result<(CompactionBatch, Uuid), CompactionError> {
        let active = self
            .active_batch()
            .await?
            .ok_or(CompactionError::NoActiveBatch)?;
        let run_id = active.run_id.ok_or(CompactionError::NoActiveBatch)?;
        match RunService::new(self.pool).stop(run_id).await {
            Ok(_) | Err(RunError::Validation(_)) => {}
            Err(error) => return Err(error.into()),
        }
        self.fail_batch(active.id, "cancelled", false).await?;
        let batch = self
            .batch(active.id)
            .await?
            .ok_or(CompactionError::BatchNotActive(active.id))?;
        Ok((batch, run_id))
    }

    pub async fn active_batch(&self) -> Result<Option<CompactionBatch>, CompactionError> {
        let sql = format!(
            "{BATCH_SELECT} WHERE b.status IN ('queued', 'running') ORDER BY b.created_at DESC, b.id DESC LIMIT 1"
        );
        let row = sqlx::query(&sql).fetch_optional(self.pool).await?;
        row.as_ref().map(row_to_batch).transpose()
    }

    pub async fn last_batch(&self) -> Result<Option<CompactionBatch>, CompactionError> {
        let sql = format!("{BATCH_SELECT} ORDER BY b.created_at DESC, b.id DESC LIMIT 1");
        let row = sqlx::query(&sql).fetch_optional(self.pool).await?;
        row.as_ref().map(row_to_batch).transpose()
    }

    pub async fn batch(&self, batch_id: Uuid) -> Result<Option<CompactionBatch>, CompactionError> {
        let sql = format!("{BATCH_SELECT} WHERE b.id = $1");
        let row = sqlx::query(&sql)
            .bind(batch_id)
            .fetch_optional(self.pool)
            .await?;
        row.as_ref().map(row_to_batch).transpose()
    }

    pub async fn queue_counts(&self) -> Result<QueueCounts, CompactionError> {
        let row = sqlx::query(
            r#"
            SELECT
                count(*) FILTER (WHERE attempts < $1) AS queued,
                count(*) FILTER (WHERE attempts >= $1) AS blocked,
                min(enqueued_at) FILTER (WHERE attempts < $1) AS oldest_queued_at
            FROM knowledge_compaction_queue
            WHERE batch_id IS NULL
            "#,
        )
        .bind(self.config.compaction.max_attempts)
        .fetch_one(self.pool)
        .await?;
        Ok(QueueCounts {
            queued: row.try_get("queued")?,
            blocked: row.try_get("blocked")?,
            oldest_queued_at: row.try_get("oldest_queued_at")?,
        })
    }

    /// When the next scheduled cycle is due, if anything is waiting.
    pub async fn next_scheduled_at(&self) -> Result<Option<OffsetDateTime>, CompactionError> {
        let counts = self.queue_counts().await?;
        Ok(next_scheduled_at(
            self.last_cycle_started_at().await?,
            counts.oldest_queued_at,
            self.interval(),
        ))
    }

    /// The configured agent, required to exist and be enabled.
    pub async fn ready_agent(&self) -> Result<Agent, CompactionError> {
        let agent = WorkspaceSettingsService::new(self.pool)
            .knowledge_compaction_agent()
            .await?
            .ok_or(CompactionError::NotConfigured)?;
        if !agent.enabled {
            return Err(CompactionError::AgentDisabled);
        }
        Ok(agent)
    }

    fn interval(&self) -> Duration {
        Duration::seconds(i64::try_from(self.config.compaction.interval_secs).unwrap_or(i64::MAX))
    }

    async fn last_cycle_started_at(&self) -> Result<Option<OffsetDateTime>, CompactionError> {
        Ok(
            sqlx::query_scalar("SELECT max(cycle_started_at) FROM knowledge_compaction_batches")
                .fetch_one(self.pool)
                .await?,
        )
    }

    /// Like [`Self::create_batch`], but losing a race for the active slot is not an error.
    async fn create_batch_or_skip(
        &self,
        agent_id: Uuid,
        trigger: BatchTrigger,
        cycle_id: Uuid,
        cycle_started_at: OffsetDateTime,
        created_by: Option<Uuid>,
    ) -> Result<Option<CompactionBatch>, CompactionError> {
        match self
            .create_batch(agent_id, trigger, cycle_id, cycle_started_at, created_by)
            .await
        {
            Err(CompactionError::ActiveBatch) => Ok(None),
            other => other,
        }
    }

    /// Claim the oldest eligible tickets enqueued before the cycle started and
    /// queue a batch plus its agent run in one transaction.
    async fn create_batch(
        &self,
        agent_id: Uuid,
        trigger: BatchTrigger,
        cycle_id: Uuid,
        cycle_started_at: OffsetDateTime,
        created_by: Option<Uuid>,
    ) -> Result<Option<CompactionBatch>, CompactionError> {
        let compaction = &self.config.compaction;
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
            WITH claimed AS (
                SELECT q.ticket_id, q.enqueued_at
                FROM knowledge_compaction_queue q
                WHERE q.batch_id IS NULL
                  AND q.attempts < $1
                  AND q.enqueued_at <= $2
                ORDER BY q.enqueued_at, q.ticket_id
                LIMIT $3
                FOR UPDATE OF q SKIP LOCKED
            )
            SELECT
                c.ticket_id,
                (
                    octet_length(t.title)
                    + octet_length(COALESCE(t.description, ''))
                    + COALESCE((
                        SELECT sum(octet_length(comment.body))
                        FROM ticket_comments comment
                        WHERE comment.ticket_id = c.ticket_id
                    ), 0)
                )::bigint AS source_bytes
            FROM claimed c
            JOIN tickets t ON t.id = c.ticket_id
            ORDER BY c.enqueued_at, c.ticket_id
            "#,
        )
        .bind(compaction.max_attempts)
        .bind(cycle_started_at)
        .bind(i64::try_from(compaction.batch_max_tickets).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await?;
        let candidates = rows
            .iter()
            .map(|row| Ok((row.try_get("ticket_id")?, row.try_get("source_bytes")?)))
            .collect::<Result<Vec<(Uuid, i64)>, sqlx::Error>>()?;
        let ticket_ids = select_within_bytes(&candidates, compaction.batch_max_source_bytes);
        if ticket_ids.is_empty() {
            tx.rollback().await?;
            return Ok(None);
        }

        let batch_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO knowledge_compaction_batches (
                id, agent_id, status, trigger, cycle_id, cycle_started_at,
                ticket_count, created_by
            ) VALUES ($1, $2, 'queued', $3, $4, $5, $6, $7)
            "#,
        )
        .bind(batch_id)
        .bind(agent_id)
        .bind(trigger.as_str())
        .bind(cycle_id)
        .bind(cycle_started_at)
        .bind(i32::try_from(ticket_ids.len()).unwrap_or(i32::MAX))
        .bind(created_by)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .and_then(|database_error| database_error.constraint())
                == Some(ONE_ACTIVE_BATCH_INDEX)
            {
                CompactionError::ActiveBatch
            } else {
                CompactionError::Database(error)
            }
        })?;
        sqlx::query(
            r#"
            INSERT INTO knowledge_compaction_batch_tickets (batch_id, ticket_id)
            SELECT $1, ticket_id FROM UNNEST($2::uuid[]) AS ticket_id
            "#,
        )
        .bind(batch_id)
        .bind(&ticket_ids)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE knowledge_compaction_queue SET batch_id = $1 WHERE ticket_id = ANY($2::uuid[])",
        )
        .bind(batch_id)
        .bind(&ticket_ids)
        .execute(&mut *tx)
        .await?;
        RunService::start_compaction_run_in_tx(&mut tx, batch_id, agent_id).await?;
        tx.commit().await?;
        self.batch(batch_id).await
    }
}

fn batch_summary(agent_summary: &str, dropped: &[String]) -> String {
    let mut summary = agent_summary.trim().to_string();
    if !dropped.is_empty() {
        if !summary.is_empty() {
            summary.push_str("\n\n");
        }
        summary.push_str(&format!("Dropped {} candidate(s):", dropped.len()));
        for reason in dropped {
            summary.push_str("\n- ");
            summary.push_str(reason);
        }
    }
    truncate_with_ellipsis(&summary, MAX_SUMMARY_BYTES)
}

fn row_to_batch(row: &sqlx::postgres::PgRow) -> Result<CompactionBatch, CompactionError> {
    let status: String = row.try_get("status")?;
    let trigger: String = row.try_get("trigger")?;
    Ok(CompactionBatch {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        agent_name: row.try_get("agent_name")?,
        run_id: row.try_get("run_id")?,
        status: BatchStatus::parse(&status).ok_or_else(|| {
            sqlx::Error::Decode(format!("unknown compaction batch status {status}").into())
        })?,
        trigger: BatchTrigger::parse(&trigger).ok_or_else(|| {
            sqlx::Error::Decode(format!("unknown compaction batch trigger {trigger}").into())
        })?,
        cycle_id: row.try_get("cycle_id")?,
        cycle_started_at: row.try_get("cycle_started_at")?,
        ticket_count: row.try_get("ticket_count")?,
        candidate_count: row.try_get("candidate_count")?,
        summary: row.try_get("summary")?,
        error_message: row.try_get("error_message")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
    })
}
