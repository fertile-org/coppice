use crate::domain::chat_message::{ChatMessage, ChatMessageRole};
use crate::domain::chat_session::{ChatSession, ChatSessionStatus};
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::project_service::{ProjectError, ProjectService};
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::run_service::{RunError, RunService};
use crate::domain::run::AgentRun;
use sqlx::PgPool;
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

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

impl From<ProjectError> for ChatError {
    fn from(err: ProjectError) -> Self {
        match err {
            ProjectError::ProjectNotFound => ChatError::Validation("project not found".into()),
            ProjectError::RepoNotFound => ChatError::Validation("repo not found".into()),
            ProjectError::Database(e) => ChatError::Database(e),
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

#[derive(Debug, Clone)]
pub struct PostMessageResult {
    pub message: ChatMessage,
    pub run: AgentRun,
}

impl<'a> ChatService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        owner_user_id: Uuid,
        agent_id: Uuid,
        project_id: Option<Uuid>,
        repo_id: Option<Uuid>,
    ) -> Result<ChatSession, ChatError> {
        let agent = AgentService::new(self.pool).get(agent_id).await?;
        if !agent.enabled {
            return Err(ChatError::Validation("agent is disabled".into()));
        }
        if let Some(project_id) = project_id {
            ProjectService::new(self.pool).get_project(project_id).await?;
        }
        if let Some(repo_id) = repo_id {
            RepoService::new(self.pool).get(repo_id).await?;
        }

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO chat_sessions (
                id, project_id, owner_user_id, agent_id, repo_id, status
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id, project_id, owner_user_id, agent_id, repo_id, status,
                created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(owner_user_id)
        .bind(agent_id)
        .bind(repo_id)
        .bind(ChatSessionStatus::Active.as_str())
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_session(&row))
    }

    pub async fn list_sessions(
        &self,
        owner_user_id: Uuid,
        project_id: Option<Uuid>,
    ) -> Result<Vec<ChatSession>, ChatError> {
        let rows = if let Some(project_id) = project_id {
            sqlx::query(
                r#"
                SELECT
                    id, project_id, owner_user_id, agent_id, repo_id, status,
                    created_at, updated_at
                FROM chat_sessions
                WHERE owner_user_id = $1 AND project_id = $2
                ORDER BY updated_at DESC
                "#,
            )
            .bind(owner_user_id)
            .bind(project_id)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT
                    id, project_id, owner_user_id, agent_id, repo_id, status,
                    created_at, updated_at
                FROM chat_sessions
                WHERE owner_user_id = $1
                ORDER BY updated_at DESC
                "#,
            )
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
        let row = sqlx::query(
            r#"
            SELECT
                id, project_id, owner_user_id, agent_id, repo_id, status,
                created_at, updated_at
            FROM chat_sessions
            WHERE id = $1 AND owner_user_id = $2
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ChatError::NotFound)?;
        Ok(row_to_session(&row))
    }

    pub async fn get_session_by_id(&self, session_id: Uuid) -> Result<ChatSession, ChatError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, project_id, owner_user_id, agent_id, repo_id, status,
                created_at, updated_at
            FROM chat_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ChatError::NotFound)?;
        Ok(row_to_session(&row))
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
                id, project_id, owner_user_id, agent_id, repo_id, status,
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
                id, session_id, seq, role, body, agent_run_id, action_metadata, created_at
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
    ) -> Result<PostMessageResult, ChatError> {
        let body = body.trim();
        if body.is_empty() {
            return Err(ChatError::Validation("message body is required".into()));
        }

        let session = self.get_session(session_id, owner_user_id).await?;
        if session.status != ChatSessionStatus::Active {
            return Err(ChatError::Validation(
                "session is not active; reopen before posting".into(),
            ));
        }

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
                id, session_id, seq, role, body, agent_run_id, action_metadata
            )
            VALUES ($1, $2, $3, $4, $5, NULL, NULL)
            RETURNING
                id, session_id, seq, role, body, agent_run_id, action_metadata, created_at
            "#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(next_seq)
        .bind(ChatMessageRole::Human.as_str())
        .bind(body)
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

        // Link the human message to the run for transcript reload.
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
                id, session_id, seq, role, body, agent_run_id, action_metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, NULL)
            RETURNING
                id, session_id, seq, role, body, agent_run_id, action_metadata, created_at
            "#,
        )
        .bind(id)
        .bind(session_id)
        .bind(next_seq)
        .bind(ChatMessageRole::Agent.as_str())
        .bind(body)
        .bind(run_id)
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
            SELECT role, body FROM chat_messages
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

        let mut out = String::new();
        for row in rows {
            let role: String = row.get("role");
            let body: String = row.get("body");
            let label = match role.as_str() {
                "human" => "Human",
                "agent" => "Agent",
                "system" => "System",
                other => other,
            };
            out.push_str(&format!("### {label}\n\n{body}\n\n"));
        }
        Ok(out)
    }
}

fn row_to_session(row: &sqlx::postgres::PgRow) -> ChatSession {
    let status_str: String = row.get("status");
    ChatSession {
        id: row.get("id"),
        project_id: row.get("project_id"),
        owner_user_id: row.get("owner_user_id"),
        agent_id: row.get("agent_id"),
        repo_id: row.get("repo_id"),
        status: ChatSessionStatus::from_str(&status_str).unwrap_or(ChatSessionStatus::Active),
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
        created_at: row.get("created_at"),
    }
}
