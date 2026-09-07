use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::chat_message::ChatMessage;
use crate::domain::chat_session::{ChatSession, ChatSessionStatus};
use crate::services::chat_service::{ChatError, ChatService};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    id: Uuid,
    project_id: Option<Uuid>,
    owner_user_id: Uuid,
    agent_id: Uuid,
    repo_id: Option<Uuid>,
    status: String,
    created_at: String,
    updated_at: String,
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

fn map_error(err: ChatError) -> StatusCode {
    match err {
        ChatError::NotFound => StatusCode::NOT_FOUND,
        ChatError::Validation(_) => StatusCode::BAD_REQUEST,
        ChatError::ActiveRunExists => StatusCode::CONFLICT,
        ChatError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn session_response(session: ChatSession) -> SessionResponse {
    SessionResponse {
        id: session.id,
        project_id: session.project_id,
        owner_user_id: session.owner_user_id,
        agent_id: session.agent_id,
        repo_id: session.repo_id,
        status: session.status.as_str().to_string(),
        created_at: session.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: session.updated_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn message_response(message: ChatMessage) -> MessageResponse {
    MessageResponse {
        id: message.id,
        session_id: message.session_id,
        seq: message.seq,
        role: message.role.as_str().to_string(),
        body: message.body,
        agent_run_id: message.agent_run_id,
        action_metadata: message.action_metadata,
        created_at: message.created_at.format(&Rfc3339).unwrap_or_default(),
    }
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
    let status = ChatSessionStatus::from_str(&body.status)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
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
    Ok(Json(MessageListResponse {
        messages: messages.into_iter().map(message_response).collect(),
    }))
}

async fn post_message(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<PostMessageBody>,
) -> Result<(StatusCode, Json<PostMessageResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let result = ChatService::new(pool)
        .post_message(session_id, user.id, &body.body)
        .await
        .map_err(map_error)?;
    Ok((
        StatusCode::CREATED,
        Json(PostMessageResponse {
            message: message_response(result.message),
            run_id: result.run.id,
        }),
    ))
}
