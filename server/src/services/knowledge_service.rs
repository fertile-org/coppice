use crate::config::{KnowledgeConfig, KnowledgeRetrievalConfig};
use crate::domain::knowledge::{
    confidence_from_str, confidence_to_str, scope_from_str, scope_to_str, source_type_from_str,
    source_type_to_str, status_from_str, status_to_str, type_from_str, type_to_str,
    validate_revision, KnowledgeConfidence, KnowledgeItemView, KnowledgeRevisionInput,
    KnowledgeScope, KnowledgeStatus, KnowledgeType,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

const ONE_LIVE_REPLACEMENT_INDEX: &str = "knowledge_items_one_live_replacement_idx";

const VIEW_SELECT: &str = r#"
SELECT
    i.id, i.version, i.status, i.current_revision_id, i.active_revision_id,
    i.approved_by, i.approved_at, i.approval_mode, i.policy_decision,
    i.policy_reason, i.rejection_reason, i.expires_at, i.supersedes_item_id,
    i.superseded_by, i.stale_at, i.created_at, i.updated_at,
    r.revision_number, r.scope, r.project_id, p.name AS project_name,
    r.agent_id, a.name AS agent_name, r.knowledge_type, r.title, r.content,
    r.source_type, r.source_id, r.source_run_id, r.confidence,
    CASE
        WHEN e.revision_id IS NOT NULL THEN 'ready'
        WHEN j.status = 'failed' THEN 'failed'
        WHEN j.status = 'running' THEN 'processing'
        WHEN j.status = 'pending' THEN 'pending'
        ELSE 'not_requested'
    END AS embedding_status,
    j.last_error AS embedding_error,
    (SELECT count(*) FROM knowledge_usage_logs u WHERE u.item_id = i.id) AS usage_count,
    (SELECT max(u.included_at) FROM knowledge_usage_logs u WHERE u.item_id = i.id) AS last_used_at
FROM knowledge_items i
JOIN knowledge_revisions r ON r.id = i.current_revision_id
LEFT JOIN projects p ON p.id = r.project_id
LEFT JOIN agents a ON a.id = r.agent_id
LEFT JOIN knowledge_embeddings e ON e.revision_id = r.id
LEFT JOIN knowledge_jobs j ON j.kind = 'embed_revision' AND j.revision_id = r.id
"#;

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("knowledge item not found")]
    NotFound,
    #[error("knowledge item version conflict; current version is {current_version}")]
    VersionConflict { current_version: i32 },
    #[error("invalid knowledge: {0}")]
    Validation(String),
    #[error("knowledge capacity reached: {0}")]
    Capacity(String),
    #[error("knowledge item already has a live replacement")]
    LiveReplacementConflict,
    #[error("knowledge item has already been superseded")]
    AlreadySupersededConflict,
    #[error("knowledge activation was blocked by a concurrent lifecycle change")]
    ActivationConflict,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Default)]
pub struct KnowledgeListFilter {
    pub status: Option<KnowledgeStatus>,
    pub project_id: Option<Uuid>,
    pub knowledge_type: Option<KnowledgeType>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct KnowledgePage {
    pub items: Vec<KnowledgeItemView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct KnowledgeRevisionPatch {
    pub scope: Option<KnowledgeScope>,
    pub project_id: Option<Option<Uuid>>,
    pub agent_id: Option<Option<Uuid>>,
    pub knowledge_type: Option<KnowledgeType>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub confidence: Option<KnowledgeConfidence>,
}

#[derive(Clone, Copy)]
struct LockedItem {
    version: i32,
    status: KnowledgeStatus,
    current_revision_id: Uuid,
    supersedes_item_id: Option<Uuid>,
    superseded_by: Option<Uuid>,
}

pub struct KnowledgeService<'a> {
    pool: &'a PgPool,
    config: &'a KnowledgeConfig,
}

impl<'a> KnowledgeService<'a> {
    pub fn new(pool: &'a PgPool, config: &'a KnowledgeConfig) -> Self {
        Self { pool, config }
    }

    pub async fn create_manual(
        &self,
        user_id: Uuid,
        mut input: KnowledgeRevisionInput,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        validate_revision(&mut input).map_err(KnowledgeError::Validation)?;
        let mut tx = self.pool.begin().await?;
        self.enforce_capacity(&mut tx, &input, None).await?;
        let item_id = self
            .insert_item_revision(
                &mut tx,
                user_id,
                &input,
                None,
                None,
                KnowledgeStatus::Pending,
                None,
                None,
            )
            .await?;
        tx.commit().await?;
        self.get(item_id).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_extracted(
        &self,
        extraction_job_id: Uuid,
        candidate_index: i32,
        input: KnowledgeRevisionInput,
        status: KnowledgeStatus,
        policy_decision: &str,
        policy_reason: &str,
    ) -> Result<Uuid, KnowledgeError> {
        let mut tx = self.pool.begin().await?;
        let item_id = self
            .create_extracted_in_tx(
                &mut tx,
                extraction_job_id,
                candidate_index,
                input,
                status,
                policy_decision,
                policy_reason,
            )
            .await?;
        tx.commit().await?;
        Ok(item_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_extracted_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        extraction_job_id: Uuid,
        candidate_index: i32,
        mut input: KnowledgeRevisionInput,
        status: KnowledgeStatus,
        policy_decision: &str,
        policy_reason: &str,
    ) -> Result<Uuid, KnowledgeError> {
        validate_revision(&mut input).map_err(KnowledgeError::Validation)?;
        if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM knowledge_items WHERE extraction_job_id = $1 AND extraction_candidate_index = $2",
        )
        .bind(extraction_job_id)
        .bind(candidate_index)
        .fetch_optional(&mut **tx)
        .await?
        {
            return Ok(existing);
        }
        self.enforce_capacity(tx, &input, None).await?;
        let approval_mode = (status == KnowledgeStatus::Approved).then_some("policy");
        let item_id = self
            .insert_item_revision(
                tx,
                Uuid::nil(),
                &input,
                None,
                Some((extraction_job_id, candidate_index)),
                status,
                approval_mode,
                Some((policy_decision, policy_reason)),
            )
            .await?;
        if status == KnowledgeStatus::Approved {
            let revision_id: Uuid =
                sqlx::query_scalar("SELECT current_revision_id FROM knowledge_items WHERE id = $1")
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await?;
            enqueue_embedding(tx, revision_id).await?;
        }
        Ok(item_id)
    }

    pub async fn get(&self, item_id: Uuid) -> Result<KnowledgeItemView, KnowledgeError> {
        let sql = format!("{VIEW_SELECT} WHERE i.id = $1");
        let row = sqlx::query(&sql)
            .bind(item_id)
            .fetch_optional(self.pool)
            .await?
            .ok_or(KnowledgeError::NotFound)?;
        row_to_view(&row)
    }

    pub async fn list(&self, filter: KnowledgeListFilter) -> Result<KnowledgePage, KnowledgeError> {
        let limit = filter
            .limit
            .unwrap_or(self.config.retrieval.default_page_size)
            .clamp(1, self.config.retrieval.max_page_size.min(100));
        let (cursor_at, cursor_id) = match filter.cursor.as_deref() {
            Some(cursor) => {
                let (timestamp, id) = parse_cursor(cursor)?;
                (Some(timestamp), Some(id))
            }
            None => (None, None),
        };
        let sql = format!(
            r#"{VIEW_SELECT}
WHERE ($1::text IS NULL OR i.status = $1)
  AND ($2::uuid IS NULL OR r.project_id = $2)
  AND ($3::text IS NULL OR r.knowledge_type = $3)
  AND ($4::timestamptz IS NULL OR (i.updated_at, i.id) < ($4, $5))
ORDER BY i.updated_at DESC, i.id DESC
LIMIT $6"#
        );
        let rows = sqlx::query(&sql)
            .bind(filter.status.map(status_to_str))
            .bind(filter.project_id)
            .bind(filter.knowledge_type.map(type_to_str))
            .bind(cursor_at)
            .bind(cursor_id)
            .bind((limit + 1) as i64)
            .fetch_all(self.pool)
            .await?;
        let has_more = rows.len() > limit;
        let mut items = rows
            .iter()
            .take(limit)
            .map(row_to_view)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            items
                .last()
                .map(|item| format_cursor(item.updated_at, item.id))
        } else {
            None
        };
        items.shrink_to_fit();
        Ok(KnowledgePage { items, next_cursor })
    }

    pub async fn approve(
        &self,
        item_id: Uuid,
        expected_version: i32,
        user_id: Uuid,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        let mut tx = self.pool.begin().await?;
        let item = lock_item_for_activation(&mut tx, item_id).await?;
        check_version(item, expected_version)?;
        if item.superseded_by.is_none() {
            let revision =
                sqlx::query("SELECT scope, project_id FROM knowledge_revisions WHERE id = $1")
                    .bind(item.current_revision_id)
                    .fetch_one(&mut *tx)
                    .await?;
            let scope = parse_scope(revision.try_get("scope")?)?;
            let project_id = revision.try_get("project_id")?;
            self.enforce_capacity_for_scope(&mut tx, scope, project_id, Some(item_id))
                .await?;
        }
        sqlx::query(
            r#"
            UPDATE knowledge_items
            SET status = 'approved', version = version + 1,
                approved_by = $2, approved_at = now(), approval_mode = 'human',
                rejection_reason = NULL, stale_at = NULL, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(item_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        enqueue_embedding(&mut tx, item.current_revision_id).await?;
        activate_embedded_revision(&mut tx, item_id, item.current_revision_id, self.config).await?;
        tx.commit().await?;
        self.get(item_id).await
    }

    pub async fn edit(
        &self,
        item_id: Uuid,
        expected_version: i32,
        user_id: Uuid,
        patch: KnowledgeRevisionPatch,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        let mut tx = self.pool.begin().await?;
        let item = lock_item(&mut tx, item_id).await?;
        check_version(item, expected_version)?;
        let row = sqlx::query(
            r#"
            SELECT revision_number, scope, project_id, agent_id, knowledge_type,
                   title, content, source_type, source_id, source_run_id, confidence
            FROM knowledge_revisions WHERE id = $1
            "#,
        )
        .bind(item.current_revision_id)
        .fetch_one(&mut *tx)
        .await?;
        let mut input = KnowledgeRevisionInput {
            scope: patch.scope.unwrap_or(parse_scope(row.try_get("scope")?)?),
            project_id: patch.project_id.unwrap_or(row.try_get("project_id")?),
            agent_id: patch.agent_id.unwrap_or(row.try_get("agent_id")?),
            knowledge_type: patch
                .knowledge_type
                .unwrap_or(parse_type(row.try_get("knowledge_type")?)?),
            title: patch.title.unwrap_or(row.try_get("title")?),
            content: patch.content.unwrap_or(row.try_get("content")?),
            source_type: parse_source_type(row.try_get("source_type")?)?,
            source_id: row.try_get("source_id")?,
            source_run_id: row.try_get("source_run_id")?,
            confidence: patch
                .confidence
                .unwrap_or(parse_confidence(row.try_get("confidence")?)?),
        };
        validate_revision(&mut input).map_err(KnowledgeError::Validation)?;
        if item.superseded_by.is_none()
            && matches!(
                item.status,
                KnowledgeStatus::Pending | KnowledgeStatus::Approved
            )
        {
            self.enforce_capacity(&mut tx, &input, Some(item_id))
                .await?;
        }
        let revision_id = insert_revision(
            &mut tx,
            item_id,
            row.try_get::<i32, _>("revision_number")? + 1,
            user_id,
            &input,
        )
        .await?;
        sqlx::query(
            "UPDATE knowledge_items SET current_revision_id = $2, version = version + 1, updated_at = now() WHERE id = $1",
        )
        .bind(item_id)
        .bind(revision_id)
        .execute(&mut *tx)
        .await?;
        if item.status == KnowledgeStatus::Approved {
            enqueue_embedding(&mut tx, revision_id).await?;
        }
        tx.commit().await?;
        self.get(item_id).await
    }

    pub async fn reject(
        &self,
        item_id: Uuid,
        expected_version: i32,
        reason: Option<&str>,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        self.simple_lifecycle(item_id, expected_version, "rejected", reason.map(str::trim))
            .await
    }

    pub async fn mark_stale(
        &self,
        item_id: Uuid,
        expected_version: i32,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        self.simple_lifecycle(item_id, expected_version, "stale", None)
            .await
    }

    pub async fn expire(
        &self,
        item_id: Uuid,
        expected_version: i32,
        expires_at: OffsetDateTime,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        let mut tx = self.pool.begin().await?;
        let item = lock_item(&mut tx, item_id).await?;
        check_version(item, expected_version)?;
        sqlx::query(
            "UPDATE knowledge_items SET expires_at = $2, version = version + 1, updated_at = now() WHERE id = $1",
        )
        .bind(item_id)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get(item_id).await
    }

    pub async fn supersede(
        &self,
        item_id: Uuid,
        expected_version: i32,
        user_id: Uuid,
        mut replacement: KnowledgeRevisionInput,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        validate_revision(&mut replacement).map_err(KnowledgeError::Validation)?;
        let mut tx = self.pool.begin().await?;
        let item = lock_item(&mut tx, item_id).await?;
        check_version(item, expected_version)?;
        if item.superseded_by.is_some() {
            return Err(KnowledgeError::AlreadySupersededConflict);
        }
        let has_live_replacement: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM knowledge_items
                WHERE supersedes_item_id = $1
                  AND status IN ('pending', 'approved')
            )
            "#,
        )
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await?;
        if has_live_replacement {
            return Err(KnowledgeError::LiveReplacementConflict);
        }
        self.enforce_capacity(&mut tx, &replacement, None).await?;
        let replacement_id = self
            .insert_item_revision(
                &mut tx,
                user_id,
                &replacement,
                Some(item_id),
                None,
                KnowledgeStatus::Pending,
                None,
                None,
            )
            .await?;
        sqlx::query(
            "UPDATE knowledge_items SET version = version + 1, updated_at = now() WHERE id = $1",
        )
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get(replacement_id).await
    }

    async fn simple_lifecycle(
        &self,
        item_id: Uuid,
        expected_version: i32,
        status: &str,
        reason: Option<&str>,
    ) -> Result<KnowledgeItemView, KnowledgeError> {
        let mut tx = self.pool.begin().await?;
        let item = lock_item(&mut tx, item_id).await?;
        check_version(item, expected_version)?;
        let stale_at = (status == "stale").then_some(OffsetDateTime::now_utc());
        sqlx::query(
            r#"
            UPDATE knowledge_items
            SET status = $2, version = version + 1, active_revision_id = NULL,
                rejection_reason = CASE WHEN $2 = 'rejected' THEN $3 ELSE rejection_reason END,
                stale_at = COALESCE($4, stale_at), updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(item_id)
        .bind(status)
        .bind(reason)
        .bind(stale_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get(item_id).await
    }

    async fn enforce_capacity(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        input: &KnowledgeRevisionInput,
        exclude_item_id: Option<Uuid>,
    ) -> Result<(), KnowledgeError> {
        self.enforce_capacity_for_scope(tx, input.scope, input.project_id, exclude_item_id)
            .await
    }

    async fn enforce_capacity_for_scope(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: KnowledgeScope,
        project_id: Option<Uuid>,
        exclude_item_id: Option<Uuid>,
    ) -> Result<(), KnowledgeError> {
        enforce_capacity_for_scope(
            tx,
            &self.config.retrieval,
            scope,
            project_id,
            exclude_item_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_item_revision(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        input: &KnowledgeRevisionInput,
        supersedes_item_id: Option<Uuid>,
        extraction: Option<(Uuid, i32)>,
        status: KnowledgeStatus,
        approval_mode: Option<&str>,
        policy: Option<(&str, &str)>,
    ) -> Result<Uuid, KnowledgeError> {
        let item_id = Uuid::new_v4();
        let revision_id = Uuid::new_v4();
        let created_by = (user_id != Uuid::nil()).then_some(user_id);
        let (policy_decision, policy_reason) = policy.unzip();
        let (extraction_job_id, extraction_candidate_index) = extraction.unzip();
        sqlx::query(
            r#"
            INSERT INTO knowledge_items (
                id, status, approved_at, approval_mode, policy_decision, policy_reason,
                supersedes_item_id, created_by, extraction_job_id, extraction_candidate_index
            ) VALUES (
                $1, $2, CASE WHEN $2 = 'approved' THEN now() END, $3, $4, $5,
                $6, $7, $8, $9
            )
            "#,
        )
        .bind(item_id)
        .bind(status_to_str(status))
        .bind(approval_mode)
        .bind(policy_decision)
        .bind(policy_reason)
        .bind(supersedes_item_id)
        .bind(created_by)
        .bind(extraction_job_id)
        .bind(extraction_candidate_index)
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?;
        insert_revision_with_id(tx, revision_id, item_id, 1, created_by, input).await?;
        sqlx::query("UPDATE knowledge_items SET current_revision_id = $2 WHERE id = $1")
            .bind(item_id)
            .bind(revision_id)
            .execute(&mut **tx)
            .await?;
        Ok(item_id)
    }
}

pub async fn activate_embedded_revision(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
    revision_id: Uuid,
    config: &KnowledgeConfig,
) -> Result<bool, KnowledgeError> {
    let candidate = lock_item_for_activation(tx, item_id).await?;
    if candidate.current_revision_id != revision_id || candidate.status != KnowledgeStatus::Approved
    {
        return Ok(false);
    }
    let embedding_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM knowledge_embeddings WHERE revision_id = $1)",
    )
    .bind(revision_id)
    .fetch_one(&mut **tx)
    .await?;
    if !embedding_ready {
        return Ok(false);
    }

    let revision = sqlx::query("SELECT scope, project_id FROM knowledge_revisions WHERE id = $1")
        .bind(revision_id)
        .fetch_one(&mut **tx)
        .await?;
    enforce_capacity_for_scope(
        tx,
        &config.retrieval,
        parse_scope(revision.try_get("scope")?)?,
        revision.try_get("project_id")?,
        Some(item_id),
    )
    .await?;

    if let Some(original_id) = candidate.supersedes_item_id {
        // lock_item_for_activation already holds this original row lock.
        let original = sqlx::query("SELECT superseded_by FROM knowledge_items WHERE id = $1")
            .bind(original_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(KnowledgeError::ActivationConflict)?;
        let superseded_by: Option<Uuid> = original.try_get("superseded_by")?;
        match superseded_by {
            None => {
                sqlx::query(
                    r#"
                    UPDATE knowledge_items
                    SET superseded_by = $2, version = version + 1, updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(original_id)
                .bind(item_id)
                .execute(&mut **tx)
                .await?;
            }
            Some(existing) if existing == item_id => {}
            Some(_) => return Err(KnowledgeError::LiveReplacementConflict),
        }
    }

    let activated = sqlx::query(
        r#"
        UPDATE knowledge_items
        SET active_revision_id = $2, updated_at = now()
        WHERE id = $1 AND current_revision_id = $2 AND status = 'approved'
        "#,
    )
    .bind(item_id)
    .bind(revision_id)
    .execute(&mut **tx)
    .await?;
    if activated.rows_affected() != 1 {
        return Err(KnowledgeError::ActivationConflict);
    }
    Ok(true)
}

async fn enforce_capacity_for_scope(
    tx: &mut Transaction<'_, Postgres>,
    config: &KnowledgeRetrievalConfig,
    scope: KnowledgeScope,
    project_id: Option<Uuid>,
    exclude_item_id: Option<Uuid>,
) -> Result<(), KnowledgeError> {
    sqlx::query("LOCK TABLE knowledge_items IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **tx)
        .await?;
    match scope {
        KnowledgeScope::Workspace => {
            let count: i64 = sqlx::query_scalar(
                r#"
                SELECT count(*)
                FROM knowledge_items i
                JOIN knowledge_revisions current_r ON current_r.id = i.current_revision_id
                LEFT JOIN knowledge_revisions active_r ON active_r.id = i.active_revision_id
                WHERE i.superseded_by IS NULL
                  AND ($1::uuid IS NULL OR i.id <> $1)
                  AND (
                        (
                            (i.status = 'pending' OR (
                                i.status = 'approved'
                                AND (i.expires_at IS NULL OR i.expires_at > now())
                            ))
                            AND current_r.scope = 'workspace'
                        )
                        OR (
                            i.status = 'approved'
                            AND (i.expires_at IS NULL OR i.expires_at > now())
                            AND active_r.scope = 'workspace'
                        )
                      )
                "#,
            )
            .bind(exclude_item_id)
            .fetch_one(&mut **tx)
            .await?;
            if count >= config.max_active_workspace {
                return Err(KnowledgeError::Capacity(format!(
                    "workspace limit {} reached",
                    config.max_active_workspace
                )));
            }
        }
        KnowledgeScope::Project | KnowledgeScope::Agent => {
            let project_id = project_id
                .ok_or_else(|| KnowledgeError::Validation("projectId is required".into()))?;
            let count: i64 = sqlx::query_scalar(
                r#"
                SELECT count(*)
                FROM knowledge_items i
                JOIN knowledge_revisions current_r ON current_r.id = i.current_revision_id
                LEFT JOIN knowledge_revisions active_r ON active_r.id = i.active_revision_id
                WHERE i.superseded_by IS NULL
                  AND ($2::uuid IS NULL OR i.id <> $2)
                  AND (
                        (
                            (i.status = 'pending' OR (
                                i.status = 'approved'
                                AND (i.expires_at IS NULL OR i.expires_at > now())
                            ))
                            AND current_r.project_id = $1
                        )
                        OR (
                            i.status = 'approved'
                            AND (i.expires_at IS NULL OR i.expires_at > now())
                            AND active_r.project_id = $1
                        )
                      )
                "#,
            )
            .bind(project_id)
            .bind(exclude_item_id)
            .fetch_one(&mut **tx)
            .await?;
            if count >= config.max_active_per_project {
                return Err(KnowledgeError::Capacity(format!(
                    "project limit {} reached",
                    config.max_active_per_project
                )));
            }
        }
    }
    Ok(())
}

fn map_database_error(error: sqlx::Error) -> KnowledgeError {
    if error
        .as_database_error()
        .and_then(|database_error| database_error.constraint())
        == Some(ONE_LIVE_REPLACEMENT_INDEX)
    {
        KnowledgeError::LiveReplacementConflict
    } else {
        KnowledgeError::Database(error)
    }
}

async fn lock_item(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
) -> Result<LockedItem, KnowledgeError> {
    let row = sqlx::query(
        "SELECT version, status, current_revision_id, supersedes_item_id, superseded_by FROM knowledge_items WHERE id = $1 FOR UPDATE",
    )
    .bind(item_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(KnowledgeError::NotFound)?;
    Ok(LockedItem {
        version: row.try_get("version")?,
        status: parse_status(row.try_get("status")?)?,
        current_revision_id: row.try_get("current_revision_id")?,
        supersedes_item_id: row.try_get("supersedes_item_id")?,
        superseded_by: row.try_get("superseded_by")?,
    })
}

async fn lock_item_for_activation(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
) -> Result<LockedItem, KnowledgeError> {
    let discovered = sqlx::query("SELECT supersedes_item_id FROM knowledge_items WHERE id = $1")
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(KnowledgeError::NotFound)?;
    let supersedes_item_id: Option<Uuid> = discovered.try_get("supersedes_item_id")?;

    if let Some(original_id) = supersedes_item_id {
        sqlx::query("SELECT id FROM knowledge_items WHERE id = $1 FOR UPDATE")
            .bind(original_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(KnowledgeError::ActivationConflict)?;
    }

    let item = lock_item(tx, item_id).await?;
    if item.supersedes_item_id != supersedes_item_id {
        return Err(KnowledgeError::ActivationConflict);
    }
    Ok(item)
}

fn check_version(item: LockedItem, expected: i32) -> Result<(), KnowledgeError> {
    if item.version != expected {
        return Err(KnowledgeError::VersionConflict {
            current_version: item.version,
        });
    }
    Ok(())
}

async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
    revision_number: i32,
    user_id: Uuid,
    input: &KnowledgeRevisionInput,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    insert_revision_with_id(tx, id, item_id, revision_number, Some(user_id), input).await?;
    Ok(id)
}

async fn insert_revision_with_id(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    item_id: Uuid,
    revision_number: i32,
    user_id: Option<Uuid>,
    input: &KnowledgeRevisionInput,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO knowledge_revisions (
            id, item_id, revision_number, scope, project_id, agent_id,
            knowledge_type, title, content, source_type, source_id,
            source_run_id, confidence, created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#,
    )
    .bind(id)
    .bind(item_id)
    .bind(revision_number)
    .bind(scope_to_str(input.scope))
    .bind(input.project_id)
    .bind(input.agent_id)
    .bind(type_to_str(input.knowledge_type))
    .bind(&input.title)
    .bind(&input.content)
    .bind(source_type_to_str(input.source_type))
    .bind(input.source_id)
    .bind(input.source_run_id)
    .bind(confidence_to_str(input.confidence))
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn enqueue_embedding(
    tx: &mut Transaction<'_, Postgres>,
    revision_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO knowledge_jobs (id, kind, revision_id)
        VALUES ($1, 'embed_revision', $2)
        ON CONFLICT (revision_id) WHERE kind = 'embed_revision' DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(revision_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn row_to_view(row: &sqlx::postgres::PgRow) -> Result<KnowledgeItemView, KnowledgeError> {
    Ok(KnowledgeItemView {
        id: row.try_get("id")?,
        version: row.try_get("version")?,
        status: parse_status(row.try_get("status")?)?,
        revision_id: row.try_get("current_revision_id")?,
        revision_number: row.try_get("revision_number")?,
        active_revision_id: row.try_get("active_revision_id")?,
        scope: parse_scope(row.try_get("scope")?)?,
        project_id: row.try_get("project_id")?,
        project_name: row.try_get("project_name")?,
        agent_id: row.try_get("agent_id")?,
        agent_name: row.try_get("agent_name")?,
        knowledge_type: parse_type(row.try_get("knowledge_type")?)?,
        title: row.try_get("title")?,
        content: row.try_get("content")?,
        source_type: parse_source_type(row.try_get("source_type")?)?,
        source_id: row.try_get("source_id")?,
        source_run_id: row.try_get("source_run_id")?,
        confidence: parse_confidence(row.try_get("confidence")?)?,
        approved_by: row.try_get("approved_by")?,
        approved_at: row.try_get("approved_at")?,
        approval_mode: row.try_get("approval_mode")?,
        policy_decision: row.try_get("policy_decision")?,
        policy_reason: row.try_get("policy_reason")?,
        rejection_reason: row.try_get("rejection_reason")?,
        expires_at: row.try_get("expires_at")?,
        supersedes_item_id: row.try_get("supersedes_item_id")?,
        superseded_by: row.try_get("superseded_by")?,
        stale_at: row.try_get("stale_at")?,
        embedding_status: row.try_get("embedding_status")?,
        embedding_error: row.try_get("embedding_error")?,
        usage_count: row.try_get("usage_count")?,
        last_used_at: row.try_get("last_used_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub fn format_cursor(updated_at: OffsetDateTime, id: Uuid) -> String {
    format!("{}:{id}", updated_at.unix_timestamp_nanos())
}

pub fn parse_cursor(cursor: &str) -> Result<(OffsetDateTime, Uuid), KnowledgeError> {
    let (timestamp, id) = cursor
        .split_once(':')
        .ok_or_else(|| KnowledgeError::Validation("invalid cursor".into()))?;
    let nanos = timestamp
        .parse::<i128>()
        .map_err(|_| KnowledgeError::Validation("invalid cursor".into()))?;
    let timestamp = OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|_| KnowledgeError::Validation("invalid cursor".into()))?;
    let id =
        Uuid::parse_str(id).map_err(|_| KnowledgeError::Validation("invalid cursor".into()))?;
    Ok((timestamp, id))
}

fn parse_scope(value: &str) -> Result<KnowledgeScope, KnowledgeError> {
    scope_from_str(value).ok_or_else(|| KnowledgeError::Validation("invalid scope".into()))
}
fn parse_type(value: &str) -> Result<KnowledgeType, KnowledgeError> {
    type_from_str(value).ok_or_else(|| KnowledgeError::Validation("invalid type".into()))
}
fn parse_source_type(
    value: &str,
) -> Result<crate::domain::knowledge::KnowledgeSourceType, KnowledgeError> {
    source_type_from_str(value)
        .ok_or_else(|| KnowledgeError::Validation("invalid source type".into()))
}
fn parse_confidence(value: &str) -> Result<KnowledgeConfidence, KnowledgeError> {
    confidence_from_str(value)
        .ok_or_else(|| KnowledgeError::Validation("invalid confidence".into()))
}
fn parse_status(value: &str) -> Result<KnowledgeStatus, KnowledgeError> {
    status_from_str(value).ok_or_else(|| KnowledgeError::Validation("invalid status".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip_is_stable() {
        let at = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let id = Uuid::new_v4();
        let parsed = parse_cursor(&format_cursor(at, id)).unwrap();
        assert_eq!(parsed, (at, id));
    }

    #[test]
    fn malformed_cursor_is_validation_error() {
        assert!(matches!(
            parse_cursor("not-a-cursor"),
            Err(KnowledgeError::Validation(_))
        ));
    }
}
