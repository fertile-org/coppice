use crate::api::auth::{pool_from_state, AuthUser};
use crate::api::knowledge::item_response as knowledge_item_response;
use crate::api::tickets::ticket_to_response;
use crate::domain::attachment::Attachment;
use crate::domain::chat_message::ChatMessage;
use crate::domain::chat_session::{ChatSession, ChatSessionStatus};
use crate::services::chat_service::{
    ChatError, ChatService, CreateKnowledgeFromChatInput, CreateTicketFromChatInput,
};
use crate::services::comment_service::CommentService;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
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
            "/api/chat/sessions/{session_id}/create-ticket",
            post(create_ticket),
        )
        .route(
            "/api/chat/sessions/{session_id}/create-knowledge",
            post(create_knowledge),
        )
        .route("/api/chat/sessions/{session_id}/cutoff", post(cutoff_session))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionBody {
    agent_id: Uuid,
    project_id: Option<Uuid>,
    repo_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSessionsQuery {
    project_id: Option<Uuid>,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTicketBody {
    project_id: Uuid,
    title: Option<String>,
    description: Option<String>,
    repo_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateKnowledgeBody {
    title: Option<String>,
    content: Option<String>,
    knowledge_type: Option<String>,
    scope: Option<String>,
    project_id: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    id: Uuid,
    project_id: Option<Uuid>,
    owner_user_id: Uuid,
    agent_id: Uuid,
    repo_id: Option<Uuid>,
    parent_session_id: Option<Uuid>,
    status: String,
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostMessageResponse {
    message: MessageResponse,
    run_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTicketResponse {
    ticket: crate::api::tickets::TicketResponse,
    message: MessageResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateKnowledgeResponse {
    knowledge: crate::api::knowledge::KnowledgeResponse,
    message: MessageResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CutoffResponse {
    parent: SessionResponse,
    child: SessionResponse,
    seed_message: MessageResponse,
}

fn map_error(err: ChatError) -> StatusCode {
    match err {
        ChatError::NotFound => StatusCode::NOT_FOUND,
        ChatError::Validation(_) => StatusCode::BAD_REQUEST,
        ChatError::ActiveRunExists => StatusCode::CONFLICT,
        ChatError::Database(_) | ChatError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn session_response(session: ChatSession) -> SessionResponse {
    SessionResponse {
        id: session.id,
        project_id: session.project_id,
        owner_user_id: session.owner_user_id,
        agent_id: session.agent_id,
        repo_id: session.repo_id,
        parent_session_id: session.parent_session_id,
        status: session.status.as_str().to_string(),
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
) -> Result<(StatusCode, Json<SessionResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let session = ChatService::new(pool)
        .create_session(user.id, body.agent_id, body.project_id, body.repo_id)
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(session_response(session))))
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let sessions = ChatService::new(pool)
        .list_sessions(user.id, query.project_id)
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
    Ok(Json(session_response(session)))
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<MessageListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let messages = ChatService::new(pool)
        .list_messages(session_id, user.id)
        .await
        .map_err(map_error)?;
    let attachments_by_id = attachments_for_messages(pool, &messages).await?;
    Ok(Json(MessageListResponse {
        messages: messages
            .into_iter()
            .map(|m| message_response(m, &attachments_by_id))
            .collect(),
    }))
}

async fn post_message(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<PostMessageBody>,
) -> Result<(StatusCode, Json<PostMessageResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let attachment_ids = body.attachment_ids.unwrap_or_default();
    let result = ChatService::new(pool)
        .post_message(session_id, user.id, &body.body, &attachment_ids)
        .await
        .map_err(map_error)?;
    let attachments_by_id = attachments_for_messages(pool, std::slice::from_ref(&result.message)).await?;
    Ok((
        StatusCode::CREATED,
        Json(PostMessageResponse {
            message: message_response(result.message, &attachments_by_id),
            run_id: result.run.id,
        }),
    ))
}

async fn create_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<CreateTicketBody>,
) -> Result<(StatusCode, Json<CreateTicketResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let result = ChatService::new(pool)
        .create_ticket_from_chat(
            session_id,
            user.id,
            CreateTicketFromChatInput {
                project_id: body.project_id,
                title: body.title.as_deref(),
                description: body.description.as_deref(),
                repo_id: body.repo_id,
                created_by: &user.email,
            },
        )
        .await
        .map_err(map_error)?;
    let attachments_by_id =
        attachments_for_messages(pool, std::slice::from_ref(&result.system_message)).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateTicketResponse {
            ticket: ticket_to_response(result.ticket),
            message: message_response(result.system_message, &attachments_by_id),
        }),
    ))
}

async fn create_knowledge(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<CreateKnowledgeBody>,
) -> Result<(StatusCode, Json<CreateKnowledgeResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let result = ChatService::new(pool)
        .create_knowledge_from_chat(
            session_id,
            user.id,
            &state.config.knowledge,
            CreateKnowledgeFromChatInput {
                title: body.title.as_deref(),
                content: body.content.as_deref(),
                knowledge_type: body.knowledge_type.as_deref(),
                scope: body.scope.as_deref(),
                project_id: body.project_id,
            },
        )
        .await
        .map_err(map_error)?;
    let attachments_by_id =
        attachments_for_messages(pool, std::slice::from_ref(&result.system_message)).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateKnowledgeResponse {
            knowledge: knowledge_item_response(result.item),
            message: message_response(result.system_message, &attachments_by_id),
        }),
    ))
}

async fn cutoff_session(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<CutoffResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let result = ChatService::new(pool)
        .cutoff_session(session_id, user.id)
        .await
        .map_err(map_error)?;
    let attachments_by_id =
        attachments_for_messages(pool, std::slice::from_ref(&result.seed_message)).await?;
    Ok(Json(CutoffResponse {
        parent: session_response(result.parent),
        child: session_response(result.child),
        seed_message: message_response(result.seed_message, &attachments_by_id),
    }))
}
