use crate::domain::attachment::Attachment;
use crate::domain::chat_action::{
    compact_transcript, draft_ticket_title, proposal_mut, ChatActionResultMeta,
};
use crate::domain::chat_message::{ChatMessage, ChatMessageRole};
use crate::domain::chat_session::{title_from_first_message, ChatSession, ChatSessionStatus};
use crate::services::chat_transcript::bound_rendered_transcript;
use crate::domain::knowledge::{
    scope_from_str, type_from_str, KnowledgeConfidence, KnowledgeItemView, KnowledgeRevisionInput,
    KnowledgeScope, KnowledgeSourceType, KnowledgeType, MAX_KNOWLEDGE_CONTENT_CHARS,
};
use crate::domain::run::AgentRun;
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::comment_service::CommentService;
use crate::services::knowledge_service::{KnowledgeError, KnowledgeService};
use crate::services::board_service::{BoardError, BoardService};
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::storage::attachment_store::sanitize_filename;
use coppice_config::KnowledgeConfig;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

const TRANSCRIPT_COMPACT_CHARS: usize = 4_000;
pub const CHAT_MESSAGE_PREVIEW_MAX_LEN: usize = 120;

/// Collapse whitespace and truncate to [`CHAT_MESSAGE_PREVIEW_MAX_LEN`] chars for session list previews.
pub fn truncate_message_preview(body: &str) -> String {
    let collapsed = body
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.chars().count() <= CHAT_MESSAGE_PREVIEW_MAX_LEN {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(CHAT_MESSAGE_PREVIEW_MAX_LEN).collect();
    out.push('…');
    out
}
const MAX_CHAT_ATTACHMENTS_PER_MESSAGE: usize = 5;

/// MIME types allowed when associating uploads with a chat message (stricter than comments).
pub const CHAT_ATTACHMENT_MIME_ALLOWLIST: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "text/plain",
    "text/markdown",
    "text/csv",
    "application/json",
    "application/pdf",
];

const TRANSCRIPT_ROWS_ASC: &str = r#"
    SELECT seq, role, body, attachment_ids FROM chat_messages
    WHERE session_id = $1
    ORDER BY seq ASC
"#;

pub struct ChatService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("chat session not found")]
    NotFound,
    #[error("validation error: {0}")]
    Validation(String),
    /// Chat is off for this connector. The string is the user-facing refusal.
    #[error("{0}")]
    ChatUnavailable(String),
    #[error("active run already exists")]
    ActiveRunExists,
    /// The proposal was already accepted or rejected.
    #[error("proposal is no longer pending")]
    ProposalNotPending,
    /// Accepting a ticket needs a board from the override, the proposal or the session.
    #[error("boardId is required to create a ticket")]
    BoardRequired,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<AgentError> for ChatError {
    fn from(err: AgentError) -> Self {
        match err {
            AgentError::AgentNotFound => ChatError::Validation("agent not found".into()),
            AgentError::Validation(msg) => ChatError::Validation(msg),
            AgentError::Database(e) => ChatError::Database(e),
            other => ChatError::Validation(other.to_string()),
        }
    }
}

impl From<BoardError> for ChatError {
    fn from(err: BoardError) -> Self {
        match err {
            BoardError::BoardNotFound => ChatError::Validation("board not found".into()),
            BoardError::RepoNotFound => ChatError::Validation("repo not found".into()),
            BoardError::InvalidName => ChatError::Validation(err.to_string()),
            BoardError::NameConflict => ChatError::Validation(err.to_string()),
            BoardError::Database(e) => ChatError::Database(e),
        }
    }
}

impl From<RepoError> for ChatError {
    fn from(err: RepoError) -> Self {
        match err {
            RepoError::NotFound => ChatError::Validation("repo not found".into()),
            RepoError::Validation(msg) => ChatError::Validation(msg),
            RepoError::Database(e) => ChatError::Database(e),
            other => ChatError::Validation(other.to_string()),
        }
    }
}

impl From<RunError> for ChatError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::ActiveRunExists => ChatError::ActiveRunExists,
            RunError::NotFound => ChatError::NotFound,
            RunError::Validation(msg) => ChatError::Validation(msg),
            RunError::Database(e) => ChatError::Database(e),
        }
    }
}

impl From<TicketError> for ChatError {
    fn from(err: TicketError) -> Self {
        match err {
            TicketError::TicketNotFound => ChatError::NotFound,
            TicketError::BoardNotFound => ChatError::Validation("board not found".into()),
            TicketError::Validation(msg) => ChatError::Validation(msg),
            TicketError::Database(e) => ChatError::Database(e),
            other => ChatError::Validation(other.to_string()),
        }
    }
}

impl From<KnowledgeError> for ChatError {
    fn from(err: KnowledgeError) -> Self {
        match err {
            KnowledgeError::NotFound => ChatError::NotFound,
            KnowledgeError::Validation(msg) => ChatError::Validation(msg),
            KnowledgeError::Database(e) => ChatError::Database(e),
            other => ChatError::Validation(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostMessageResult {
    pub message: ChatMessage,
    pub run: AgentRun,
}

pub struct CreateTicketFromChatResult {
    pub ticket: TicketWithDisplay,
    pub system_message: ChatMessage,
}

#[derive(Debug, Clone)]
pub struct CreateKnowledgeFromChatResult {
    pub item: KnowledgeItemView,
    pub system_message: ChatMessage,
}

pub struct CreateTicketFromChatInput<'a> {
    pub board_id: Uuid,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub repo_id: Option<Uuid>,
    pub created_by: &'a str,
}

pub struct CreateKnowledgeFromChatInput<'a> {
    pub title: Option<&'a str>,
    pub content: Option<&'a str>,
    pub knowledge_type: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub board_id: Option<Uuid>,
}

/// Optional overrides for an accepted proposal. Unset fields use the proposal.
pub struct AcceptProposalInput<'a> {
    pub title: Option<&'a str>,
    /// Ticket description, or knowledge content.
    pub description: Option<&'a str>,
    pub board_id: Option<Uuid>,
    pub knowledge_type: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub created_by: &'a str,
}

pub struct AcceptedProposal {
    /// Agent message with the proposal marked `created`.
    pub message: ChatMessage,
    /// The "Created ticket" or "Proposed knowledge" note.
    pub system_message: ChatMessage,
    pub ticket: Option<TicketWithDisplay>,
    pub knowledge: Option<KnowledgeItemView>,
}

fn ensure_chat_available(connector: &str) -> Result<(), ChatError> {
    if let Some(message) = crate::providers::chat_unavailable_message(connector) {
        return Err(ChatError::ChatUnavailable(message));
    }
    Ok(())
}

impl<'a> ChatService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        owner_user_id: Uuid,
        agent_id: Uuid,
        board_id: Option<Uuid>,
        repo_id: Option<Uuid>,
    ) -> Result<ChatSession, ChatError> {
        self.create_session_inner(owner_user_id, agent_id, board_id, repo_id, None)
            .await
    }

    async fn create_session_inner(
        &self,
        owner_user_id: Uuid,
        agent_id: Uuid,
        board_id: Option<Uuid>,
        repo_id: Option<Uuid>,
        parent_session_id: Option<Uuid>,
    ) -> Result<ChatSession, ChatError> {
        let agent = AgentService::new(self.pool).get(agent_id).await?;
        if !agent.enabled {
            return Err(ChatError::Validation("agent is disabled".into()));
        }
        ensure_chat_available(&agent.connector)?;
        if let Some(board_id) = board_id {
            BoardService::new(self.pool).get_board(board_id).await?;
        }
        if let Some(repo_id) = repo_id {
            RepoService::new(self.pool).get(repo_id).await?;
        }

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO chat_sessions (
                id, board_id, owner_user_id, agent_id, repo_id, parent_session_id, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING
                id, board_id, owner_user_id, agent_id, repo_id, parent_session_id, status,
                provider_session_id, provider_session_connector,
                created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(owner_user_id)
        .bind(agent_id)
        .bind(repo_id)
        .bind(parent_session_id)
        .bind(ChatSessionStatus::Active.as_str())
        .fetch_one(self.pool)
        .await?;

        Ok(session_from_insert_row(&row))
    }

    pub async fn list_sessions(
        &self,
        owner_user_id: Uuid,
        board_id: Option<Uuid>,
    ) -> Result<Vec<ChatSession>, ChatError> {
        let rows = if let Some(board_id) = board_id {
            sqlx::query(&format!(
                r#"
                SELECT {CHAT_SESSION_LIST_SELECT}
                FROM chat_sessions cs
                LEFT JOIN LATERAL (
                    SELECT body, role, created_at
                    FROM chat_messages
                    WHERE session_id = cs.id
                    ORDER BY seq DESC
                    LIMIT 1
                ) lm ON true
                LEFT JOIN LATERAL (
                    SELECT body AS first_human_body
                    FROM chat_messages
                    WHERE session_id = cs.id
                      AND role = 'human'
                      AND btrim(body) <> ''
                    ORDER BY seq ASC
                    LIMIT 1
                ) fm ON true
                WHERE cs.owner_user_id = $1 AND cs.board_id = $2
                ORDER BY cs.updated_at DESC
                "#,
            ))
            .bind(owner_user_id)
            .bind(board_id)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                r#"
                SELECT {CHAT_SESSION_LIST_SELECT}
                FROM chat_sessions cs
                LEFT JOIN LATERAL (
                    SELECT body, role, created_at
                    FROM chat_messages
                    WHERE session_id = cs.id
                    ORDER BY seq DESC
                    LIMIT 1
                ) lm ON true
                LEFT JOIN LATERAL (
                    SELECT body AS first_human_body
                    FROM chat_messages
                    WHERE session_id = cs.id
                      AND role = 'human'
                      AND btrim(body) <> ''
                    ORDER BY seq ASC
                    LIMIT 1
                ) fm ON true
                WHERE cs.owner_user_id = $1
                ORDER BY cs.updated_at DESC
                "#,
            ))
            .bind(owner_user_id)
            .fetch_all(self.pool)
            .await?
        };

        Ok(rows.iter().map(row_to_session).collect())
    }

    pub async fn get_session(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<ChatSession, ChatError> {
        let row = sqlx::query(&format!(
            r#"
            SELECT {CHAT_SESSION_LIST_SELECT}
            FROM chat_sessions cs
            LEFT JOIN LATERAL (
                SELECT body, role, created_at
                FROM chat_messages
                WHERE session_id = cs.id
                ORDER BY seq DESC
                LIMIT 1
            ) lm ON true
            LEFT JOIN LATERAL (
                SELECT body AS first_human_body
                FROM chat_messages
                WHERE session_id = cs.id
                  AND role = 'human'
                  AND btrim(body) <> ''
                ORDER BY seq ASC
                LIMIT 1
            ) fm ON true
            WHERE cs.id = $1 AND cs.owner_user_id = $2
            "#,
        ))
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ChatError::NotFound)?;
        Ok(row_to_session(&row))
    }

    pub async fn get_session_by_id(&self, session_id: Uuid) -> Result<ChatSession, ChatError> {
        let row = sqlx::query(&format!(
            r#"
            SELECT {CHAT_SESSION_LIST_SELECT}
            FROM chat_sessions cs
            LEFT JOIN LATERAL (
                SELECT body, role, created_at
                FROM chat_messages
                WHERE session_id = cs.id
                ORDER BY seq DESC
                LIMIT 1
            ) lm ON true
            LEFT JOIN LATERAL (
                SELECT body AS first_human_body
                FROM chat_messages
                WHERE session_id = cs.id
                  AND role = 'human'
                  AND btrim(body) <> ''
                ORDER BY seq ASC
                LIMIT 1
            ) fm ON true
            WHERE cs.id = $1
            "#,
        ))
        .bind(session_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ChatError::NotFound)?;
        Ok(row_to_session(&row))
    }

    pub async fn set_provider_session(
        &self,
        session_id: Uuid,
        connector: &str,
        provider_session_id: &str,
    ) -> Result<(), ChatError> {
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET provider_session_id = $2,
                provider_session_connector = $3,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .bind(provider_session_id)
        .bind(connector)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_provider_session(&self, session_id: Uuid) -> Result<(), ChatError> {
        sqlx::query(
            r#"
            UPDATE chat_sessions
            SET provider_session_id = NULL,
                provider_session_connector = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// The message as a resumed chat context shows it: body plus the same
    /// `Attachments:` list the transcript renders. The bool is whether the
    /// message has attachments.
    pub async fn render_message(&self, message_id: Uuid) -> Result<(String, bool), ChatError> {
        let row = sqlx::query(r#"SELECT body, attachment_ids FROM chat_messages WHERE id = $1"#)
            .bind(message_id)
            .fetch_optional(self.pool)
            .await?
            .ok_or(ChatError::NotFound)?;
        let body: String = row.get("body");
        let attachment_ids: Vec<Uuid> = row.get("attachment_ids");
        let by_id = self.attachments_by_id(&attachment_ids).await?;
        let has_attachments = !attachment_ids.is_empty();
        Ok((render_message_text(&body, &attachment_ids, &by_id), has_attachments))
    }

    async fn attachments_by_id(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Attachment>, ChatError> {
        let attachments = CommentService::new(self.pool)
            .list_attachments_by_ids(attachment_ids)
            .await
            .map_err(|err| match err {
                crate::services::comment_service::CommentError::Database(e) => {
                    ChatError::Database(e)
                }
                other => ChatError::Validation(other.to_string()),
            })?;
        Ok(attachments.into_iter().map(|a| (a.id, a)).collect())
    }

    pub async fn patch_session(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        status: ChatSessionStatus,
    ) -> Result<ChatSession, ChatError> {
        let _ = self.get_session(session_id, owner_user_id).await?;
        if !matches!(
            status,
            ChatSessionStatus::Active | ChatSessionStatus::Archived
        ) {
            return Err(ChatError::Validation("invalid session status".into()));
        }
        let row = sqlx::query(
            r#"
            UPDATE chat_sessions
            SET status = $2, updated_at = now()
            WHERE id = $1 AND owner_user_id = $3
            RETURNING
                id, board_id, owner_user_id, agent_id, repo_id, parent_session_id, status,
                provider_session_id, provider_session_connector,
                created_at, updated_at
            "#,
        )
        .bind(session_id)
        .bind(status.as_str())
        .bind(owner_user_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_session(&row))
    }

    pub async fn list_messages(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<Vec<ChatMessage>, ChatError> {
        let _ = self.get_session(session_id, owner_user_id).await?;
        let rows = sqlx::query(
            r#"
            SELECT
                id, session_id, seq, role, body, agent_run_id, action_metadata,
                attachment_ids, created_at
            FROM chat_messages
            WHERE session_id = $1
            ORDER BY seq ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(row_to_message).collect())
    }

    /// The newest `limit` messages before `before_seq` (or overall), oldest
    /// first, and the seq to pass for the next older page when any remain.
    pub async fn list_messages_page(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        before_seq: Option<i64>,
        limit: i64,
    ) -> Result<(Vec<ChatMessage>, Option<i64>), ChatError> {
        let _ = self.get_session(session_id, owner_user_id).await?;
        let limit = limit.max(1);
        let rows = sqlx::query(
            r#"
            SELECT
                id, session_id, seq, role, body, agent_run_id, action_metadata,
                attachment_ids, created_at
            FROM chat_messages
            WHERE session_id = $1 AND ($2::BIGINT IS NULL OR seq < $2)
            ORDER BY seq DESC
            LIMIT $3
            "#,
        )
        .bind(session_id)
        .bind(before_seq)
        .bind(limit + 1)
        .fetch_all(self.pool)
        .await?;
        let has_more = rows.len() as i64 > limit;
        let mut messages: Vec<ChatMessage> =
            rows.iter().take(limit as usize).map(row_to_message).collect();
        messages.reverse();
        let older = has_more.then(|| messages.first().map(|m| m.seq)).flatten();
        Ok((messages, older))
    }

    pub async fn post_message(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        body: &str,
        attachment_ids: &[Uuid],
    ) -> Result<PostMessageResult, ChatError> {
        let body = body.trim();
        if body.is_empty() && attachment_ids.is_empty() {
            return Err(ChatError::Validation(
                "message body or at least one attachment is required".into(),
            ));
        }

        let session = self.get_session(session_id, owner_user_id).await?;
        if session.status != ChatSessionStatus::Active {
            return Err(ChatError::Validation(
                "session is not active; reopen before posting".into(),
            ));
        }
        let agent = AgentService::new(self.pool).get(session.agent_id).await?;
        ensure_chat_available(&agent.connector)?;
        self.validate_chat_attachments(owner_user_id, attachment_ids)
            .await?;

        let mut tx = self.pool.begin().await?;
        lock_chat_session(&mut tx, session_id).await?;
        let next_seq: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_messages WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await?;

        let message_id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO chat_messages (
                id, session_id, seq, role, body, agent_run_id, action_metadata, attachment_ids
            )
            VALUES ($1, $2, $3, $4, $5, NULL, NULL, $6)
            RETURNING
                id, session_id, seq, role, body, agent_run_id, action_metadata,
                attachment_ids, created_at
            "#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(next_seq)
        .bind(ChatMessageRole::Human.as_str())
        .bind(body)
        .bind(attachment_ids)
        .fetch_one(&mut *tx)
        .await?;
        let message = row_to_message(&row);

        sqlx::query(
            r#"
            UPDATE chat_sessions SET updated_at = now() WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let run = RunService::new(self.pool)
            .start_chat_turn(session_id, message_id, session.agent_id)
            .await?;

        sqlx::query(
            r#"
            UPDATE chat_messages SET agent_run_id = $2 WHERE id = $1
            "#,
        )
        .bind(message_id)
        .bind(run.id)
        .execute(self.pool)
        .await?;

        let message = ChatMessage {
            agent_run_id: Some(run.id),
            ..message
        };

        Ok(PostMessageResult { message, run })
    }

    /// Agent reply that carries pending proposals (see `proposals_metadata`).
    pub async fn append_agent_message_with_proposals(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        body: &str,
        proposals: Option<serde_json::Value>,
    ) -> Result<ChatMessage, ChatError> {
        self.append_message(session_id, ChatMessageRole::Agent, body, Some(run_id), proposals)
            .await
    }

    pub async fn append_system_note(
        &self,
        session_id: Uuid,
        body: &str,
    ) -> Result<ChatMessage, ChatError> {
        self.append_message(session_id, ChatMessageRole::System, body, None, None)
            .await
    }

    async fn append_message(
        &self,
        session_id: Uuid,
        role: ChatMessageRole,
        body: &str,
        agent_run_id: Option<Uuid>,
        action_metadata: Option<serde_json::Value>,
    ) -> Result<ChatMessage, ChatError> {
        let mut tx = self.pool.begin().await?;
        lock_chat_session(&mut tx, session_id).await?;
        let next_seq: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_messages WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await?;

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO chat_messages (
                id, session_id, seq, role, body, agent_run_id, action_metadata, attachment_ids
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, '{}')
            RETURNING
                id, session_id, seq, role, body, agent_run_id, action_metadata,
                attachment_ids, created_at
            "#,
        )
        .bind(id)
        .bind(session_id)
        .bind(next_seq)
        .bind(role.as_str())
        .bind(body)
        .bind(agent_run_id)
        .bind(action_metadata)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE chat_sessions SET updated_at = now() WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row_to_message(&row))
    }

    pub async fn format_transcript(&self, session_id: Uuid) -> Result<String, ChatError> {
        let blocks = self.transcript_blocks(session_id).await?;
        if blocks.is_empty() {
            return Ok("(No previous messages)".into());
        }
        Ok(blocks.concat())
    }

    /// Transcript sent to the model. Older turns are summarized once the
    /// rendered text would pass `max_tokens`.
    pub async fn format_model_transcript(
        &self,
        session_id: Uuid,
        max_tokens: usize,
    ) -> Result<String, ChatError> {
        Ok(self
            .format_model_transcript_with_attachments(session_id, max_tokens)
            .await?
            .0)
    }

    /// Like `format_model_transcript`, and also whether any rendered turn has
    /// attachments.
    pub async fn format_model_transcript_with_attachments(
        &self,
        session_id: Uuid,
        max_tokens: usize,
    ) -> Result<(String, bool), ChatError> {
        let rows = self.recent_transcript_rows(session_id, max_tokens).await?;
        let has_attachments = rows.iter().any(|row| {
            let ids: Vec<Uuid> = row.get("attachment_ids");
            !ids.is_empty()
        });
        let blocks = self.render_blocks(rows).await?;
        Ok((bound_rendered_transcript(&blocks, max_tokens), has_attachments))
    }

    /// The newest messages the bounded transcript can use, oldest first. Rows
    /// are read newest first in pages until the recent part is past the
    /// budget and enough older rows are there for every summary line that
    /// could fit, so a long chat is not loaded whole.
    async fn recent_transcript_rows(
        &self,
        session_id: Uuid,
        max_tokens: usize,
    ) -> Result<Vec<sqlx::postgres::PgRow>, ChatError> {
        const PAGE: i64 = 200;
        // A summary line is at least ~12 bytes ("- Human: x").
        const MIN_SUMMARY_LINE_BYTES: usize = 12;
        if max_tokens == 0 {
            return Ok(sqlx::query(TRANSCRIPT_ROWS_ASC)
                .bind(session_id)
                .fetch_all(self.pool)
                .await?);
        }
        let byte_budget = max_tokens.saturating_mul(4);
        let summary_rows_needed = byte_budget / MIN_SUMMARY_LINE_BYTES + 1;
        let mut newest_first = Vec::new();
        let mut bytes = 0usize;
        let mut rows_past_budget = 0usize;
        let mut before: Option<i64> = None;
        loop {
            let page = sqlx::query(
                r#"
                SELECT seq, role, body, attachment_ids FROM chat_messages
                WHERE session_id = $1 AND ($2::BIGINT IS NULL OR seq < $2)
                ORDER BY seq DESC
                LIMIT $3
                "#,
            )
            .bind(session_id)
            .bind(before)
            .bind(PAGE)
            .fetch_all(self.pool)
            .await?;
            let short = (page.len() as i64) < PAGE;
            for row in page {
                before = Some(row.get::<i64, _>("seq"));
                if bytes >= byte_budget {
                    rows_past_budget += 1;
                }
                bytes = bytes.saturating_add(row.get::<String, _>("body").len() + 32);
                newest_first.push(row);
            }
            if short || rows_past_budget >= summary_rows_needed {
                break;
            }
        }
        newest_first.reverse();
        Ok(newest_first)
    }

    async fn transcript_blocks(&self, session_id: Uuid) -> Result<Vec<String>, ChatError> {
        let rows = sqlx::query(TRANSCRIPT_ROWS_ASC)
            .bind(session_id)
            .fetch_all(self.pool)
            .await?;
        self.render_blocks(rows).await
    }

    async fn render_blocks(
        &self,
        rows: Vec<sqlx::postgres::PgRow>,
    ) -> Result<Vec<String>, ChatError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_ids: Vec<Uuid> = rows
            .iter()
            .flat_map(|row| {
                let ids: Vec<Uuid> = row.get("attachment_ids");
                ids
            })
            .collect();
        all_ids.sort_unstable();
        all_ids.dedup();

        let by_id = self.attachments_by_id(&all_ids).await?;

        let mut blocks = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row.get("role");
            let body: String = row.get("body");
            let attachment_ids: Vec<Uuid> = row.get("attachment_ids");
            let label = match role.as_str() {
                "human" => "Human",
                "agent" => "Agent",
                "system" => "System",
                other => other,
            };
            let mut block = format!("### {label}\n\n");
            block.push_str(&render_message_text(&body, &attachment_ids, &by_id));
            blocks.push(block);
        }
        Ok(blocks)
    }

    /// Copy session attachment bytes into `{chat_cwd}/attachments/{id}/{filename}` for agent Read.
    pub async fn stage_attachments_into_cwd(
        &self,
        session_id: Uuid,
        chat_cwd: &Path,
    ) -> Result<(), ChatError> {
        let rows = sqlx::query(
            r#"
            SELECT attachment_ids FROM chat_messages
            WHERE session_id = $1 AND cardinality(attachment_ids) > 0
            "#,
        )
        .bind(session_id)
        .fetch_all(self.pool)
        .await?;

        let mut ids: Vec<Uuid> = rows
            .iter()
            .flat_map(|row| {
                let ids: Vec<Uuid> = row.get("attachment_ids");
                ids
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Ok(());
        }

        let attachments = CommentService::new(self.pool)
            .list_attachments_by_ids(&ids)
            .await
            .map_err(|err| match err {
                crate::services::comment_service::CommentError::Database(e) => {
                    ChatError::Database(e)
                }
                other => ChatError::Validation(other.to_string()),
            })?;

        for att in attachments {
            let dest_dir = chat_cwd.join("attachments").join(att.id.to_string());
            std::fs::create_dir_all(&dest_dir)?;
            let dest = dest_dir.join(sanitize_filename(&att.filename));
            if dest.exists() {
                continue;
            }
            match std::fs::copy(&att.storage_path, &dest) {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(ChatError::Io(err)),
            }
        }
        Ok(())
    }

    async fn validate_chat_attachments(
        &self,
        owner_user_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<(), ChatError> {
        if attachment_ids.is_empty() {
            return Ok(());
        }
        if attachment_ids.len() > MAX_CHAT_ATTACHMENTS_PER_MESSAGE {
            return Err(ChatError::Validation(format!(
                "at most {MAX_CHAT_ATTACHMENTS_PER_MESSAGE} attachments per message"
            )));
        }

        let mut seen = HashSet::new();
        for id in attachment_ids {
            if !seen.insert(*id) {
                return Err(ChatError::Validation(
                    "duplicate attachment ids are not allowed".into(),
                ));
            }
        }

        let attachments = CommentService::new(self.pool)
            .list_attachments_by_ids(attachment_ids)
            .await
            .map_err(|err| match err {
                crate::services::comment_service::CommentError::Database(e) => {
                    ChatError::Database(e)
                }
                other => ChatError::Validation(other.to_string()),
            })?;

        if attachments.len() != attachment_ids.len() {
            return Err(ChatError::Validation(
                "one or more attachments were not found".into(),
            ));
        }

        for att in &attachments {
            if att.uploaded_by != owner_user_id {
                return Err(ChatError::Validation(
                    "attachment must be uploaded by the session owner".into(),
                ));
            }
            if !CHAT_ATTACHMENT_MIME_ALLOWLIST.contains(&att.content_type.as_str()) {
                return Err(ChatError::Validation(format!(
                    "content type '{}' is not allowed for chat attachments",
                    att.content_type
                )));
            }
            if let Err(msg) = verify_image_magic_bytes(att) {
                return Err(ChatError::Validation(msg));
            }
        }
        Ok(())
    }

    /// Human-confirmed create ticket from chat context. Requires an explicit board.
    pub async fn create_ticket_from_chat(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        input: CreateTicketFromChatInput<'_>,
    ) -> Result<CreateTicketFromChatResult, ChatError> {
        let session = self.get_session(session_id, owner_user_id).await?;
        BoardService::new(self.pool)
            .get_board(input.board_id)
            .await?;

        let transcript = self.format_transcript(session_id).await?;
        let title = input
            .title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| draft_ticket_title(&transcript));
        let description = input
            .description
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| compact_transcript(&transcript, TRANSCRIPT_COMPACT_CHARS));
        let repo_id = input.repo_id.or(session.repo_id);

        let ticket = TicketService::new(self.pool)
            .create_with_source(
                input.board_id,
                &title,
                &description,
                repo_id,
                None,
                input.created_by,
                owner_user_id,
                Some(session_id),
            )
            .await?;

        let meta = ChatActionResultMeta {
            action: "create_ticket".into(),
            ticket_id: Some(ticket.ticket.id),
            knowledge_item_id: None,
        };
        let system_message = self
            .append_message(
                session_id,
                ChatMessageRole::System,
                &format!("Created ticket «{title}» from this chat."),
                None,
                Some(serde_json::to_value(&meta).unwrap_or_default()),
            )
            .await?;

        if session.board_id.is_none() {
            sqlx::query(
                r#"
                UPDATE chat_sessions
                SET board_id = $2, updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(session_id)
            .bind(input.board_id)
            .execute(self.pool)
            .await?;
        }

        Ok(CreateTicketFromChatResult {
            ticket,
            system_message,
        })
    }

    /// Human-confirmed knowledge candidate from compacted chat transcript (M06 pending).
    pub async fn create_knowledge_from_chat(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        knowledge_config: &KnowledgeConfig,
        input: CreateKnowledgeFromChatInput<'_>,
    ) -> Result<CreateKnowledgeFromChatResult, ChatError> {
        let session = self.get_session(session_id, owner_user_id).await?;
        let transcript = self.format_transcript(session_id).await?;
        let compact = compact_transcript(
            &transcript,
            MAX_KNOWLEDGE_CONTENT_CHARS.min(TRANSCRIPT_COMPACT_CHARS),
        );

        let title = input
            .title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| draft_ticket_title(&transcript));
        let content = input
            .content
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or(compact);

        let knowledge_type = match input.knowledge_type {
            Some(value) => type_from_str(value)
                .ok_or_else(|| ChatError::Validation("invalid knowledgeType".into()))?,
            None => KnowledgeType::CodingConvention,
        };

        let scope_str = input.scope.unwrap_or("board");
        let scope = scope_from_str(scope_str)
            .ok_or_else(|| ChatError::Validation("invalid scope".into()))?;

        let board_id = match scope {
            KnowledgeScope::Workspace => None,
            KnowledgeScope::Board | KnowledgeScope::Agent => {
                let board_id = input.board_id.or(session.board_id).ok_or_else(|| {
                    ChatError::Validation(
                        "boardId is required for board-scoped knowledge".into(),
                    )
                })?;
                BoardService::new(self.pool)
                    .get_board(board_id)
                    .await?;
                Some(board_id)
            }
        };

        let agent_id = if scope == KnowledgeScope::Agent {
            Some(session.agent_id)
        } else {
            None
        };

        let revision = KnowledgeRevisionInput {
            scope,
            board_id,
            agent_id,
            knowledge_type,
            title: title.clone(),
            content,
            source_type: KnowledgeSourceType::ChatSession,
            source_id: Some(session_id),
            source_run_id: None,
            confidence: KnowledgeConfidence::Medium,
        };

        let item = KnowledgeService::new(self.pool, knowledge_config)
            .create_manual(owner_user_id, revision)
            .await?;

        let meta = ChatActionResultMeta {
            action: "create_knowledge".into(),
            ticket_id: None,
            knowledge_item_id: Some(item.id),
        };
        let system_message = self
            .append_message(
                session_id,
                ChatMessageRole::System,
                &format!("Proposed knowledge «{title}» (pending inbox approval)."),
                None,
                Some(serde_json::to_value(&meta).unwrap_or_default()),
            )
            .await?;

        Ok(CreateKnowledgeFromChatResult {
            item,
            system_message,
        })
    }

    /// Accept a pending proposal. The agent message row lock serializes
    /// concurrent accept/reject calls, so a proposal creates at most one item.
    pub async fn accept_proposal(
        &self,
        session_id: Uuid,
        message_id: Uuid,
        proposal_id: Uuid,
        owner_user_id: Uuid,
        knowledge_config: &KnowledgeConfig,
        input: AcceptProposalInput<'_>,
    ) -> Result<AcceptedProposal, ChatError> {
        let session = self.get_session(session_id, owner_user_id).await?;
        let mut tx = self.pool.begin().await?;
        let mut metadata = lock_proposal_message(&mut tx, session_id, message_id).await?;
        let pid = proposal_id.to_string();
        let proposal = proposal_mut(&mut metadata, &pid)
            .ok_or(ChatError::NotFound)?
            .clone();
        ensure_proposal_pending(&proposal)?;

        let str_field = |key: &str| proposal.get(key).and_then(serde_json::Value::as_str);
        let uuid_field = |key: &str| str_field(key).and_then(|s| Uuid::parse_str(s).ok());
        let override_or = |value: Option<&str>, key: &str| {
            value
                .map(str::to_string)
                .or_else(|| str_field(key).map(str::to_string))
        };

        let (key, created_id, system_message, ticket, knowledge) = match str_field("type") {
            Some("create_ticket") => {
                let board_id = input
                    .board_id
                    .or_else(|| uuid_field("boardId"))
                    .or(session.board_id)
                    .ok_or(ChatError::BoardRequired)?;
                let title = override_or(input.title, "title");
                let description = override_or(input.description, "description");
                let created = self
                    .create_ticket_from_chat(
                        session_id,
                        owner_user_id,
                        CreateTicketFromChatInput {
                            board_id,
                            title: title.as_deref(),
                            description: description.as_deref(),
                            repo_id: uuid_field("repoId"),
                            created_by: input.created_by,
                        },
                    )
                    .await?;
                (
                    "ticketId",
                    created.ticket.ticket.id,
                    created.system_message,
                    Some(created.ticket),
                    None,
                )
            }
            Some("create_knowledge") => {
                let title = override_or(input.title, "title");
                let content = override_or(input.description, "content");
                let knowledge_type = override_or(input.knowledge_type, "knowledgeType");
                let scope = override_or(input.scope, "scope");
                let created = self
                    .create_knowledge_from_chat(
                        session_id,
                        owner_user_id,
                        knowledge_config,
                        CreateKnowledgeFromChatInput {
                            title: title.as_deref(),
                            content: content.as_deref(),
                            knowledge_type: knowledge_type.as_deref(),
                            scope: scope.as_deref(),
                            board_id: input.board_id.or_else(|| uuid_field("boardId")),
                        },
                    )
                    .await?;
                (
                    "knowledgeItemId",
                    created.item.id,
                    created.system_message,
                    None,
                    Some(created.item),
                )
            }
            _ => return Err(ChatError::Validation("unknown proposal type".into())),
        };

        let proposal = proposal_mut(&mut metadata, &pid).ok_or(ChatError::NotFound)?;
        proposal["status"] = "created".into();
        proposal[key] = created_id.to_string().into();
        let message = update_message_metadata(&mut tx, message_id, &metadata).await?;
        tx.commit().await?;

        Ok(AcceptedProposal {
            message,
            system_message,
            ticket,
            knowledge,
        })
    }

    /// Reject a pending proposal. Creates nothing.
    pub async fn reject_proposal(
        &self,
        session_id: Uuid,
        message_id: Uuid,
        proposal_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<ChatMessage, ChatError> {
        self.get_session(session_id, owner_user_id).await?;
        let mut tx = self.pool.begin().await?;
        let mut metadata = lock_proposal_message(&mut tx, session_id, message_id).await?;
        let pid = proposal_id.to_string();
        let proposal = proposal_mut(&mut metadata, &pid).ok_or(ChatError::NotFound)?;
        ensure_proposal_pending(proposal)?;
        proposal["status"] = "rejected".into();
        let message = update_message_metadata(&mut tx, message_id, &metadata).await?;
        tx.commit().await?;
        Ok(message)
    }
}

/// Lock one agent message's proposal metadata for the rest of `tx`.
async fn lock_proposal_message(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: Uuid,
    message_id: Uuid,
) -> Result<serde_json::Value, ChatError> {
    let row: Option<Option<serde_json::Value>> = sqlx::query_scalar(
        r#"
        SELECT action_metadata FROM chat_messages
        WHERE id = $1 AND session_id = $2 AND role = 'agent'
        FOR UPDATE
        "#,
    )
    .bind(message_id)
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.flatten().ok_or(ChatError::NotFound)
}

fn ensure_proposal_pending(proposal: &serde_json::Value) -> Result<(), ChatError> {
    match proposal.get("status").and_then(serde_json::Value::as_str) {
        Some("pending") => Ok(()),
        _ => Err(ChatError::ProposalNotPending),
    }
}

async fn update_message_metadata(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message_id: Uuid,
    metadata: &serde_json::Value,
) -> Result<ChatMessage, ChatError> {
    let row = sqlx::query(
        r#"
        UPDATE chat_messages SET action_metadata = $2 WHERE id = $1
        RETURNING
            id, session_id, seq, role, body, agent_run_id, action_metadata,
            attachment_ids, created_at
        "#,
    )
    .bind(message_id)
    .bind(metadata)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row_to_message(&row))
}

/// Serialize message inserts for one session. `seq` is unique per session, and
/// a repository-copy note can land while the person is posting.
async fn lock_chat_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT id FROM chat_sessions WHERE id = $1 FOR UPDATE")
        .bind(session_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// A message's text after its heading: the body, then one `Attachments:` list
/// with the path each file is staged at.
fn render_message_text(
    body: &str,
    attachment_ids: &[Uuid],
    by_id: &std::collections::HashMap<Uuid, Attachment>,
) -> String {
    let mut text = String::new();
    if !body.is_empty() {
        text.push_str(body);
        text.push_str("\n\n");
    }
    if !attachment_ids.is_empty() {
        text.push_str("Attachments:\n");
        for id in attachment_ids {
            if let Some(att) = by_id.get(id) {
                let safe_name = sanitize_filename(&att.filename);
                text.push_str(&format!(
                    "- file: attachments/{}/{safe_name} ({}, {} bytes)\n",
                    att.id, att.content_type, att.size_bytes
                ));
            } else {
                text.push_str(&format!("- file: attachments/{id}/(missing)\n"));
            }
        }
        text.push('\n');
    }
    text
}

fn verify_image_magic_bytes(att: &Attachment) -> Result<(), String> {
    let needs_check = matches!(
        att.content_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    );
    if !needs_check {
        return Ok(());
    }

    let bytes = std::fs::read(&att.storage_path).map_err(|_| {
        format!(
            "could not read attachment '{}' for content verification",
            att.filename
        )
    })?;

    let matches = match att.content_type.as_str() {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => {
            bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
        _ => true,
    };

    if !matches {
        return Err(format!(
            "attachment '{}' content does not match declared type {}",
            att.filename, att.content_type
        ));
    }
    Ok(())
}

const CHAT_SESSION_LIST_SELECT: &str = r"
    cs.id, cs.board_id, cs.owner_user_id, cs.agent_id, cs.repo_id, cs.parent_session_id, cs.status,
    cs.provider_session_id, cs.provider_session_connector,
    cs.created_at, cs.updated_at,
    fm.first_human_body,
    COALESCE(lm.body, '') AS last_message_body,
    lm.role AS last_message_role,
    lm.created_at AS last_message_at,
    EXISTS (
        SELECT 1 FROM agent_runs ar
        WHERE ar.chat_session_id = cs.id
          AND ar.status IN ('queued', 'running')
    ) AS has_active_run,
    (
        SELECT ar.id FROM agent_runs ar
        WHERE ar.chat_session_id = cs.id
          AND ar.status IN ('queued', 'running')
        ORDER BY ar.created_at DESC
        LIMIT 1
    ) AS active_run_id
";

fn session_from_insert_row(row: &sqlx::postgres::PgRow) -> ChatSession {
    let status_str: String = row.get("status");
    ChatSession {
        id: row.get("id"),
        board_id: row.get("board_id"),
        owner_user_id: row.get("owner_user_id"),
        agent_id: row.get("agent_id"),
        repo_id: row.get("repo_id"),
        parent_session_id: row.try_get("parent_session_id").ok().flatten(),
        status: ChatSessionStatus::from_str(&status_str).unwrap_or(ChatSessionStatus::Active),
        provider_session_id: row.try_get("provider_session_id").ok().flatten(),
        provider_session_connector: row.try_get("provider_session_connector").ok().flatten(),
        last_message_preview: String::new(),
        last_message_at: None,
        last_message_role: None,
        has_active_run: false,
        active_run_id: None,
        title: None,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_session(row: &sqlx::postgres::PgRow) -> ChatSession {
    let status_str: String = row.get("status");
    let last_message_body: String = row
        .try_get("last_message_body")
        .unwrap_or_else(|_| String::new());
    ChatSession {
        id: row.get("id"),
        board_id: row.get("board_id"),
        owner_user_id: row.get("owner_user_id"),
        agent_id: row.get("agent_id"),
        repo_id: row.get("repo_id"),
        parent_session_id: row.try_get("parent_session_id").ok().flatten(),
        status: ChatSessionStatus::from_str(&status_str).unwrap_or(ChatSessionStatus::Active),
        provider_session_id: row.try_get("provider_session_id").ok().flatten(),
        provider_session_connector: row.try_get("provider_session_connector").ok().flatten(),
        last_message_preview: truncate_message_preview(&last_message_body),
        last_message_at: row.try_get("last_message_at").ok().flatten(),
        last_message_role: row.try_get("last_message_role").ok().flatten(),
        has_active_run: row.try_get("has_active_run").unwrap_or(false),
        active_run_id: row.try_get("active_run_id").ok().flatten(),
        title: row
            .try_get::<Option<String>, _>("first_human_body")
            .ok()
            .flatten()
            .as_deref()
            .and_then(title_from_first_message),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_message(row: &sqlx::postgres::PgRow) -> ChatMessage {
    let role_str: String = row.get("role");
    ChatMessage {
        id: row.get("id"),
        session_id: row.get("session_id"),
        seq: row.get("seq"),
        role: ChatMessageRole::from_str(&role_str).unwrap_or(ChatMessageRole::System),
        body: row.get("body"),
        agent_run_id: row.get("agent_run_id"),
        action_metadata: row.get("action_metadata"),
        attachment_ids: row
            .try_get("attachment_ids")
            .unwrap_or_else(|_| Vec::new()),
        created_at: row.get("created_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_message_preview_caps_at_120() {
        let long = "a".repeat(150);
        let preview = truncate_message_preview(&long);
        assert!(preview.chars().count() <= 121);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn chat_mime_allowlist_excludes_html_and_svg() {
        assert!(CHAT_ATTACHMENT_MIME_ALLOWLIST.contains(&"image/png"));
        assert!(CHAT_ATTACHMENT_MIME_ALLOWLIST.contains(&"text/plain"));
        assert!(!CHAT_ATTACHMENT_MIME_ALLOWLIST.contains(&"text/html"));
        assert!(!CHAT_ATTACHMENT_MIME_ALLOWLIST.contains(&"image/svg+xml"));
    }

    #[tokio::test]
    async fn create_ticket_from_chat_defaults_priority_to_medium() {
        let pool = crate::db::shared_test_pool().await.expect("pool");
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'member')",
        )
        .bind(user_id)
        .bind(format!("chat-pri-{user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("user");

        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("chat priority board")
            .bind(format!("chat-pri-{board_id}"))
            .execute(&pool)
            .await
            .expect("board");

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
        .bind("Chat Agent")
        .bind("Backend Engineer")
        .bind("prompt")
        .bind("mock")
        .bind("backend_engineer")
        .execute(&pool)
        .await
        .expect("agent");

        let session_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO chat_sessions (id, board_id, owner_user_id, agent_id, status)
            VALUES ($1, $2, $3, $4, 'active')
            "#,
        )
        .bind(session_id)
        .bind(board_id)
        .bind(user_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("session");

        let created = ChatService::new(&pool)
            .create_ticket_from_chat(
                session_id,
                user_id,
                CreateTicketFromChatInput {
                    board_id,
                    title: Some("From chat"),
                    description: Some("Created from chat"),
                    repo_id: None,
                    created_by: "ada",
                },
            )
            .await
            .expect("create from chat");
        assert_eq!(
            created.ticket.ticket.priority,
            Some(crate::domain::ticket::TicketPriority::Medium)
        );
    }
}
