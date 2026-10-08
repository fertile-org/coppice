use crate::domain::attachment::Attachment;
use crate::domain::chat_action::{
    compact_transcript, draft_fields_from_agent_summary, draft_ticket_title,
    fallback_draft_ticket, ChatActionResultMeta, DraftTicketFields, DraftTicketSource,
};
use crate::domain::chat_message::{ChatMessage, ChatMessageRole};
use crate::domain::chat_session::{ChatSession, ChatSessionStatus};
use crate::domain::context_profile::ContextProfile;
use crate::domain::knowledge::{
    scope_from_str, type_from_str, KnowledgeConfidence, KnowledgeItemView, KnowledgeRevisionInput,
    KnowledgeScope, KnowledgeSourceType, KnowledgeType, MAX_KNOWLEDGE_CONTENT_CHARS,
};
use crate::domain::run::AgentRun;
use crate::domain::slug::slugify;
use crate::providers::{AgentRunInput, AgentRunResult};
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::chat_cwd::resolve_chat_cwd;
use crate::services::comment_service::CommentService;
use crate::services::context_builder::{
    build_draft_ticket_context, write_context_document, ContextInput,
};
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
use std::time::Duration;
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
const DRAFT_TICKET_TIMEOUT: Duration = Duration::from_secs(45);
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

pub struct ChatService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("chat session not found")]
    NotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error("active run already exists")]
    ActiveRunExists,
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

#[derive(Debug, Clone)]
pub struct CutoffResult {
    pub parent: ChatSession,
    pub child: ChatSession,
    pub seed_message: ChatMessage,
}

pub struct CreateTicketFromChatInput<'a> {
    pub board_id: Uuid,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub repo_id: Option<Uuid>,
    pub created_by: &'a str,
}

#[derive(Debug, Clone)]
pub struct DraftTicketResult {
    pub title: String,
    pub description: String,
    pub source: DraftTicketSource,
}

pub struct DraftTicketDeps<'a> {
    pub worktrees_path: &'a Path,
    pub artifacts_dir: Option<&'a str>,
    pub connector_registry: &'a crate::providers::ConnectorRegistry,
    pub timeout: Duration,
}

pub struct CreateKnowledgeFromChatInput<'a> {
    pub title: Option<&'a str>,
    pub content: Option<&'a str>,
    pub knowledge_type: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub board_id: Option<Uuid>,
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

    pub async fn get_message_body(&self, message_id: Uuid) -> Result<String, ChatError> {
        let body: Option<String> = sqlx::query_scalar(
            r#"SELECT body FROM chat_messages WHERE id = $1"#,
        )
        .bind(message_id)
        .fetch_optional(self.pool)
        .await?;
        body.ok_or(ChatError::NotFound)
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
            ChatSessionStatus::Active | ChatSessionStatus::Archived | ChatSessionStatus::Cutoff
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

        self.validate_chat_attachments(owner_user_id, attachment_ids)
            .await?;

        let mut tx = self.pool.begin().await?;
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

    pub async fn append_agent_message(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        body: &str,
    ) -> Result<ChatMessage, ChatError> {
        self.append_message(
            session_id,
            ChatMessageRole::Agent,
            body,
            Some(run_id),
            None,
        )
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
        let rows = sqlx::query(
            r#"
            SELECT role, body, attachment_ids FROM chat_messages
            WHERE session_id = $1
            ORDER BY seq ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(self.pool)
        .await?;

        if rows.is_empty() {
            return Ok("(No previous messages)".into());
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

        let attachments = CommentService::new(self.pool)
            .list_attachments_by_ids(&all_ids)
            .await
            .map_err(|err| match err {
                crate::services::comment_service::CommentError::Database(e) => {
                    ChatError::Database(e)
                }
                other => ChatError::Validation(other.to_string()),
            })?;
        let by_id: std::collections::HashMap<Uuid, Attachment> = attachments
            .into_iter()
            .map(|a| (a.id, a))
            .collect();

        let mut out = String::new();
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
            out.push_str(&format!("### {label}\n\n"));
            if !body.is_empty() {
                out.push_str(&body);
                out.push_str("\n\n");
            }
            if !attachment_ids.is_empty() {
                out.push_str("Attachments:\n");
                for id in &attachment_ids {
                    if let Some(att) = by_id.get(id) {
                        let safe_name = sanitize_filename(&att.filename);
                        out.push_str(&format!(
                            "- file: attachments/{}/{safe_name} ({}, {} bytes)\n",
                            att.id, att.content_type, att.size_bytes
                        ));
                    } else {
                        out.push_str(&format!("- file: attachments/{id}/(missing)\n"));
                    }
                }
                out.push('\n');
            }
        }
        Ok(out)
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
            child_session_id: None,
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

    /// In-process agent draft for create-ticket confirm dialog. Never creates a
    /// ticket or appends chat messages. Always returns a usable draft.
    pub async fn draft_ticket_from_chat(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
        deps: DraftTicketDeps<'_>,
    ) -> Result<DraftTicketResult, ChatError> {
        let session = self.get_session(session_id, owner_user_id).await?;
        let transcript = self.format_transcript(session_id).await?;
        let fallback = fallback_draft_ticket(&transcript, TRANSCRIPT_COMPACT_CHARS);

        let agent = match AgentService::new(self.pool).get(session.agent_id).await {
            Ok(agent) => agent,
            Err(_) => {
                return Ok(draft_result(fallback, DraftTicketSource::Fallback));
            }
        };
        let Some(connector) = deps.connector_registry.get(&agent.connector) else {
            return Ok(draft_result(fallback, DraftTicketSource::Fallback));
        };
        let agent_key = agent
            .preset_source
            .clone()
            .unwrap_or_else(|| slugify(&agent.name));

        let repo_local_path = if let Some(repo_id) = session.repo_id {
            sqlx::query_scalar::<_, String>("SELECT local_path FROM repos WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(self.pool)
                .await?
        } else {
            None
        };

        let cwd = match resolve_chat_cwd(deps.worktrees_path, session_id, repo_local_path.as_deref())
        {
            Ok(path) => path,
            Err(_) => {
                return Ok(draft_result(fallback, DraftTicketSource::Fallback));
            }
        };
        let cwd_str = cwd.to_string_lossy().into_owned();

        if self
            .stage_attachments_into_cwd(session_id, &cwd)
            .await
            .is_err()
        {
            // Attachments are best-effort for drafting; continue without them.
        }

        let context_input = ContextInput {
            ticket_title: "Draft ticket from chat",
            ticket_description: "",
            ticket_status: "n/a",
            ticket_substatus: None,
            agent_name: &agent.name,
            agent_key: &agent_key,
            agent_role: &agent.role,
            agent_responsibilities: &agent.responsibilities,
            agent_system_prompt: &agent.system_prompt,
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: Some(&cwd_str),
            latest_comments: Some(&transcript),
            project_rules: None,
            resume_context: None,
            context_profile: ContextProfile::Conversation,
            human_request: None,
            ticket_id: None,
            assignee_agent_key: None,
            thread_excerpt: None,
        };
        let markdown = build_draft_ticket_context(&context_input);
        if write_context_document(&cwd, &markdown).is_err() {
            return Ok(draft_result(fallback, DraftTicketSource::Fallback));
        }
        let context_path = cwd.join(".agent").join("context.md");

        let timeout = if deps.timeout.is_zero() {
            DRAFT_TICKET_TIMEOUT
        } else {
            deps.timeout
        };
        let provider_input = AgentRunInput {
            agent_id: agent.id.to_string(),
            agent_key: agent_key.clone(),
            agent_role: agent.role.clone(),
            job_type: "draft_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Conversation,
            context_path: context_path.to_string_lossy().into_owned(),
            run_id: None,
            chat_session_id: None,
            artifacts_dir: deps.artifacts_dir.map(str::to_string),
            model_provider: agent.model_provider.clone(),
            model: agent.model.clone(),
            stream: None,
            cancel_rx: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: true,
            mcp: None,
        };

        let provider_result =
            match tokio::time::timeout(timeout, connector.run(provider_input)).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) | Err(_) => {
                    return Ok(draft_result(fallback, DraftTicketSource::Fallback));
                }
            };

        let fields = match provider_result {
            AgentRunResult::Done {
                summary,
                updated_description,
                ..
            } => draft_fields_from_agent_summary(&summary, updated_description.as_deref()),
            _ => None,
        };

        match fields {
            Some(fields) => Ok(draft_result(fields, DraftTicketSource::Agent)),
            None => Ok(draft_result(fallback, DraftTicketSource::Fallback)),
        }
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
            child_session_id: None,
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

    /// Cut off the session and open a shorter child session seeded with a summary.
    pub async fn cutoff_session(
        &self,
        session_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<CutoffResult, ChatError> {
        let parent = self.get_session(session_id, owner_user_id).await?;
        if parent.status == ChatSessionStatus::Cutoff {
            return Err(ChatError::Validation("session is already cutoff".into()));
        }

        let transcript = self.format_transcript(session_id).await?;
        let summary = compact_transcript(&transcript, TRANSCRIPT_COMPACT_CHARS);

        let parent = self
            .patch_session(session_id, owner_user_id, ChatSessionStatus::Cutoff)
            .await?;
        self.clear_provider_session(session_id).await?;

        let child = self
            .create_session_inner(
                owner_user_id,
                parent.agent_id,
                parent.board_id,
                parent.repo_id,
                Some(session_id),
            )
            .await?;

        let meta = ChatActionResultMeta {
            action: "cutoff".into(),
            ticket_id: None,
            knowledge_item_id: None,
            child_session_id: Some(child.id),
        };
        let _parent_note = self
            .append_message(
                session_id,
                ChatMessageRole::System,
                &format!("Session cutoff. Continued in child session {}.", child.id),
                None,
                Some(serde_json::to_value(&meta).unwrap_or_default()),
            )
            .await?;

        let seed_body = format!(
            "Continued from cutoff session {session_id}.\n\n## Prior conversation summary\n\n{summary}"
        );
        let seed_message = self
            .append_message(
                child.id,
                ChatMessageRole::System,
                &seed_body,
                None,
                Some(serde_json::json!({
                    "action": "cutoff_seed",
                    "parentSessionId": session_id
                })),
            )
            .await?;

        Ok(CutoffResult {
            parent,
            child,
            seed_message,
        })
    }
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

fn draft_result(fields: DraftTicketFields, source: DraftTicketSource) -> DraftTicketResult {
    DraftTicketResult {
        title: fields.title,
        description: fields.description,
        source,
    }
}

const CHAT_SESSION_LIST_SELECT: &str = r"
    cs.id, cs.board_id, cs.owner_user_id, cs.agent_id, cs.repo_id, cs.parent_session_id, cs.status,
    cs.provider_session_id, cs.provider_session_connector,
    cs.created_at, cs.updated_at,
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
}
