use crate::domain::attachment::Attachment;
use crate::domain::comment::{
    author_type_from_str, author_type_to_str, intent_from_str, intent_to_str, AuthorType,
    Comment, CommentIntent,
};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct CommentService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    #[error("ticket not found")]
    TicketNotFound,
    #[error("comment not found")]
    CommentNotFound,
    #[error("attachment not found")]
    AttachmentNotFound,
    #[error("invalid intent")]
    InvalidIntent,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> CommentService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, comment_id: Uuid) -> Result<Comment, CommentError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, author_type, author_id, body, intent,
                mentions, attachment_ids, created_at
            FROM ticket_comments
            WHERE id = $1
            "#,
        )
        .bind(comment_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(CommentError::CommentNotFound)?;

        Ok(row_to_comment(&row))
    }

    pub async fn list_by_ticket(&self, ticket_id: Uuid) -> Result<Vec<Comment>, CommentError> {
        self.ensure_ticket_exists(ticket_id).await?;

        let rows = sqlx::query(
            r#"
            SELECT
                id, ticket_id, author_type, author_id, body, intent,
                mentions, attachment_ids, created_at
            FROM ticket_comments
            WHERE ticket_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_comment).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        ticket_id: Uuid,
        author_type: AuthorType,
        author_id: Option<Uuid>,
        body: &str,
        intent: CommentIntent,
        attachment_ids: &[Uuid],
        mentions: &[String],
    ) -> Result<Comment, CommentError> {
        self.ensure_ticket_exists(ticket_id).await?;

        if body.trim().is_empty() {
            return Err(CommentError::Validation("body is required".into()));
        }

        if !attachment_ids.is_empty() {
            self.ensure_attachments_exist(attachment_ids).await?;
        }

        let id = Uuid::new_v4();
        let mentions = serde_json::json!(mentions);

        let row = sqlx::query(
            r#"
            INSERT INTO ticket_comments (
                id, ticket_id, author_type, author_id, body, intent,
                mentions, attachment_ids
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING
                id, ticket_id, author_type, author_id, body, intent,
                mentions, attachment_ids, created_at
            "#,
        )
        .bind(id)
        .bind(ticket_id)
        .bind(author_type_to_str(author_type))
        .bind(author_id)
        .bind(body)
        .bind(intent_to_str(intent))
        .bind(&mentions)
        .bind(attachment_ids)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_comment(&row))
    }

    pub async fn list_attachments_by_ids(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, CommentError> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                id, filename, content_type, size_bytes, storage_path,
                uploaded_by, created_at
            FROM attachments
            WHERE id = ANY($1)
            "#,
        )
        .bind(attachment_ids)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_attachment).collect())
    }

    pub async fn get_attachment(&self, attachment_id: Uuid) -> Result<Attachment, CommentError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, filename, content_type, size_bytes, storage_path,
                uploaded_by, created_at
            FROM attachments
            WHERE id = $1
            "#,
        )
        .bind(attachment_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(CommentError::AttachmentNotFound)?;

        Ok(row_to_attachment(&row))
    }

    pub async fn create_attachment(
        &self,
        id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_path: &str,
        uploaded_by: Uuid,
    ) -> Result<Attachment, CommentError> {
        let row = sqlx::query(
            r#"
            INSERT INTO attachments (
                id, filename, content_type, size_bytes, storage_path, uploaded_by
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id, filename, content_type, size_bytes, storage_path,
                uploaded_by, created_at
            "#,
        )
        .bind(id)
        .bind(filename)
        .bind(content_type)
        .bind(size_bytes)
        .bind(storage_path)
        .bind(uploaded_by)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_attachment(&row))
    }

    async fn ensure_ticket_exists(&self, ticket_id: Uuid) -> Result<(), CommentError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tickets WHERE id = $1)",
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await?;

        if !exists {
            return Err(CommentError::TicketNotFound);
        }
        Ok(())
    }

    async fn ensure_attachments_exist(&self, attachment_ids: &[Uuid]) -> Result<(), CommentError> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM attachments WHERE id = ANY($1)",
        )
        .bind(attachment_ids)
        .fetch_one(self.pool)
        .await?;

        if count as usize != attachment_ids.len() {
            return Err(CommentError::AttachmentNotFound);
        }
        Ok(())
    }
}

fn row_to_comment(row: &sqlx::postgres::PgRow) -> Comment {
    let author_type_str: String = row.get("author_type");
    let intent_str: String = row.get("intent");

    Comment {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        author_type: author_type_from_str(&author_type_str).unwrap_or(AuthorType::Human),
        author_id: row.get("author_id"),
        body: row.get("body"),
        intent: intent_from_str(&intent_str).unwrap_or(CommentIntent::ProgressUpdate),
        mentions: row.get("mentions"),
        attachment_ids: row.get("attachment_ids"),
        created_at: row.get("created_at"),
    }
}

fn row_to_attachment(row: &sqlx::postgres::PgRow) -> Attachment {
    Attachment {
        id: row.get("id"),
        filename: row.get("filename"),
        content_type: row.get("content_type"),
        size_bytes: row.get("size_bytes"),
        storage_path: row.get("storage_path"),
        uploaded_by: row.get("uploaded_by"),
        created_at: row.get("created_at"),
    }
}
