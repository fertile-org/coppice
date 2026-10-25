use crate::api::auth::{pool_from_state, AuthUser};
use crate::api::knowledge::item_response as knowledge_item_response;
use crate::api::tickets::ticket_to_response;
use crate::domain::attachment::Attachment;
use crate::domain::chat_message::ChatMessage;
use crate::domain::chat_session::{ChatSession, ChatSessionStatus};
use crate::services::chat_service::{AcceptProposalInput, ChatError, ChatService};
use crate::services::comment_service::CommentService;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/chat/sessions", get(list_sessions).post(create_session))
        .route(
            "/api/chat/sessions/{session_id}",
            get(get_session).patch(patch_session),
        )
        .route(
            "/api/chat/sessions/{session_id}/messages",
            get(list_messages).post(post_message),
        )
        .route(
            "/api/chat/sessions/{session_id}/messages/{message_id}/proposals/{proposal_id}/accept",
            post(accept_proposal),
        )
        .route(
            "/api/chat/sessions/{session_id}/messages/{message_id}/proposals/{proposal_id}/reject",
            post(reject_proposal),
        )
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptProposalBody {
    title: Option<String>,
    /// Ticket description, or knowledge content.
    description: Option<String>,
    board_id: Option<Uuid>,
    knowledge_type: Option<String>,
    scope: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptProposalResponse {
    message: MessageResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<crate::api::tickets::TicketResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    knowledge: Option<crate::api::knowledge::KnowledgeResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionBody {
    agent_id: Uuid,
    board_id: Option<Uuid>,
    repo_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSessionsQuery {
    board_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchSessionBody {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostMessageBody {
    body: String,
    attachment_ids: Option<Vec<Uuid>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    id: Uuid,
    board_id: Option<Uuid>,
    owner_user_id: Uuid,
    agent_id: Uuid,
    repo_id: Option<Uuid>,
    parent_session_id: Option<Uuid>,
    status: String,
    last_message_preview: String,
    last_message_at: Option<String>,
    last_message_role: Option<String>,
    has_active_run: bool,
    active_run_id: Option<Uuid>,
    title: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentSummary {
    id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageResponse {
    id: Uuid,
    session_id: Uuid,
    seq: i64,
    role: String,
    body: String,
    agent_run_id: Option<Uuid>,
    action_metadata: Option<serde_json::Value>,
    attachment_ids: Vec<Uuid>,
    attachments: Vec<AttachmentSummary>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionListResponse {
    sessions: Vec<SessionResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageListResponse {
    messages: Vec<MessageResponse>,
    /// Pass as `beforeSeq` to load older messages. Absent at the start of the chat.
    #[serde(skip_serializing_if = "Option::is_none")]
    older_before_seq: Option<i64>,
}

/// Newest messages returned when the client does not ask for a page size.
const MESSAGES_PAGE_DEFAULT: i64 = 100;
const MESSAGES_PAGE_MAX: i64 = 500;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListMessagesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    before_seq: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostMessageResponse {
    message: MessageResponse,
    run_id: Uuid,
}

fn map_error(err: ChatError) -> StatusCode {
    match err {
        ChatError::NotFound => StatusCode::NOT_FOUND,
        ChatError::Validation(_) | ChatError::ChatUnavailable(_) => StatusCode::BAD_REQUEST,
        ChatError::BoardRequired => StatusCode::UNPROCESSABLE_ENTITY,
        ChatError::ActiveRunExists | ChatError::ProposalNotPending => StatusCode::CONFLICT,
        ChatError::Database(_) | ChatError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
struct ChatMessageBody {
    message: String,
}

enum ChatApiError {
    Status(StatusCode),
    Unavailable(String),
}

impl IntoResponse for ChatApiError {
    fn into_response(self) -> Response {
        match self {
            ChatApiError::Status(code) => code.into_response(),
            ChatApiError::Unavailable(message) => {
                (StatusCode::BAD_REQUEST, Json(ChatMessageBody { message })).into_response()
            }
        }
    }
}

fn map_chat_start_error(err: ChatError) -> ChatApiError {
    match err {
        ChatError::ChatUnavailable(message) => ChatApiError::Unavailable(message),
        other => ChatApiError::Status(map_error(other)),
    }
}

fn discard_chat_workspace_if_ended(state: &AppState, session_id: Uuid, status: ChatSessionStatus) {
    if !matches!(
        status,
        ChatSessionStatus::Archived
    ) {
        return;
    }
    if let Err(err) = crate::services::chat_repos::remove_chat_workspace(
        std::path::Path::new(&state.config.agent.worktrees_path),
        session_id,
    ) {
        tracing::warn!(%session_id, error = %err, "failed to remove chat folder");
    }
}

fn session_response(session: ChatSession) -> SessionResponse {
    SessionResponse {
        id: session.id,
        board_id: session.board_id,
        owner_user_id: session.owner_user_id,
        agent_id: session.agent_id,
        repo_id: session.repo_id,
        parent_session_id: session.parent_session_id,
        status: session.status.as_str().to_string(),
        last_message_preview: session.last_message_preview,
        last_message_at: session
            .last_message_at
            .map(|t| t.format(&Rfc3339).unwrap_or_default()),
        last_message_role: session.last_message_role,
        has_active_run: session.has_active_run,
        active_run_id: session.active_run_id,
        title: session.title,
        created_at: session.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: session.updated_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn attachment_to_summary(attachment: &Attachment) -> AttachmentSummary {
    AttachmentSummary {
        id: attachment.id,
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
    }
}

fn message_response(
    message: ChatMessage,
    attachments_by_id: &HashMap<Uuid, Attachment>,
) -> MessageResponse {
    let attachments = message
        .attachment_ids
        .iter()
        .filter_map(|id| attachments_by_id.get(id))
        .map(attachment_to_summary)
        .collect();

    MessageResponse {
        id: message.id,
        session_id: message.session_id,
        seq: message.seq,
        role: message.role.as_str().to_string(),
        body: message.body,
        agent_run_id: message.agent_run_id,
        action_metadata: message.action_metadata,
        attachment_ids: message.attachment_ids,
        attachments,
        created_at: message.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

async fn attachments_for_messages(
    pool: &sqlx::PgPool,
    messages: &[ChatMessage],
) -> Result<HashMap<Uuid, Attachment>, StatusCode> {
    let mut ids: Vec<Uuid> = messages
        .iter()
        .flat_map(|m| m.attachment_ids.clone())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let attachments = CommentService::new(pool)
        .list_attachments_by_ids(&ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(attachments.into_iter().map(|a| (a.id, a)).collect())
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Json(body): Json<CreateSessionBody>,
) -> Result<(StatusCode, Json<SessionResponse>), ChatApiError> {
    let pool = pool_from_state(&state).map_err(ChatApiError::Status)?;
    let session = ChatService::new(pool)
        .create_session(user.id, body.agent_id, body.board_id, body.repo_id)
        .await
        .map_err(map_chat_start_error)?;
    crate::services::chat_workspace::start_session(
        &state,
        pool.clone(),
        session.id,
        session.board_id,
        &[],
    );
    Ok((StatusCode::CREATED, Json(session_response(session))))
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let sessions = ChatService::new(pool)
        .list_sessions(user.id, query.board_id)
        .await
        .map_err(map_error)?;
    Ok(Json(SessionListResponse {
        sessions: sessions.into_iter().map(session_response).collect(),
    }))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let session = ChatService::new(pool)
        .get_session(session_id, user.id)
        .await
        .map_err(map_error)?;
    Ok(Json(session_response(session)))
}

async fn patch_session(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<PatchSessionBody>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let status = ChatSessionStatus::from_str(&body.status).map_err(|_| StatusCode::BAD_REQUEST)?;
    let session = ChatService::new(pool)
        .patch_session(session_id, user.id, status)
        .await
        .map_err(map_error)?;
    discard_chat_workspace_if_ended(&state, session_id, status);
    Ok(Json(session_response(session)))
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListMessagesQuery>,
) -> Result<Json<MessageListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let limit = query
        .limit
        .unwrap_or(MESSAGES_PAGE_DEFAULT)
        .clamp(1, MESSAGES_PAGE_MAX);
    let (messages, older_before_seq) = ChatService::new(pool)
        .list_messages_page(session_id, user.id, query.before_seq, limit)
        .await
        .map_err(map_error)?;
    let attachments_by_id = attachments_for_messages(pool, &messages).await?;
    Ok(Json(MessageListResponse {
        messages: messages
            .into_iter()
            .map(|m| message_response(m, &attachments_by_id))
            .collect(),
        older_before_seq,
    }))
}

async fn post_message(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<PostMessageBody>,
) -> Result<(StatusCode, Json<PostMessageResponse>), ChatApiError> {
    let pool = pool_from_state(&state).map_err(ChatApiError::Status)?;
    let attachment_ids = body.attachment_ids.unwrap_or_default();
    let result = ChatService::new(pool)
        .post_message(session_id, user.id, &body.body, &attachment_ids)
        .await
        .map_err(map_chat_start_error)?;
    let attachments_by_id = attachments_for_messages(pool, std::slice::from_ref(&result.message))
        .await
        .map_err(ChatApiError::Status)?;
    Ok((
        StatusCode::CREATED,
        Json(PostMessageResponse {
            message: message_response(result.message, &attachments_by_id),
            run_id: result.run.id,
        }),
    ))
}

async fn accept_proposal(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path((session_id, message_id, proposal_id)): Path<(Uuid, Uuid, Uuid)>,
    body: Option<Json<AcceptProposalBody>>,
) -> Result<Json<AcceptProposalResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let result = ChatService::new(pool)
        .accept_proposal(
            session_id,
            message_id,
            proposal_id,
            user.id,
            &state.config.knowledge,
            AcceptProposalInput {
                title: body.title.as_deref(),
                description: body.description.as_deref(),
                board_id: body.board_id,
                knowledge_type: body.knowledge_type.as_deref(),
                scope: body.scope.as_deref(),
                created_by: &user.email,
            },
        )
        .await
        .map_err(map_error)?;
    let attachments_by_id =
        attachments_for_messages(pool, std::slice::from_ref(&result.message)).await?;
    Ok(Json(AcceptProposalResponse {
        message: message_response(result.message, &attachments_by_id),
        ticket: result.ticket.map(ticket_to_response),
        knowledge: result.knowledge.map(knowledge_item_response),
    }))
}

async fn reject_proposal(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path((session_id, message_id, proposal_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let message = ChatService::new(pool)
        .reject_proposal(session_id, message_id, proposal_id, user.id)
        .await
        .map_err(map_error)?;
    let attachments_by_id =
        attachments_for_messages(pool, std::slice::from_ref(&message)).await?;
    Ok(Json(message_response(message, &attachments_by_id)))
}
