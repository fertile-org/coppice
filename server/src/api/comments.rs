use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::comment::{intent_from_str, AuthorType, Comment, CommentIntent};
use crate::services::comment_service::{CommentError, CommentService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/tickets/{ticket_id}/comments",
        get(list_comments).post(create_comment),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentResponse {
    id: Uuid,
    ticket_id: Uuid,
    author_type: String,
    author_id: Option<Uuid>,
    body: String,
    intent: String,
    mentions: serde_json::Value,
    attachment_ids: Vec<Uuid>,
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCommentBody {
    body: String,
    intent: Option<String>,
    attachment_ids: Option<Vec<Uuid>>,
    mentions: Option<Vec<String>>,
}

fn comment_to_response(comment: Comment) -> CommentResponse {
    CommentResponse {
        id: comment.id,
        ticket_id: comment.ticket_id,
        author_type: serde_json::to_value(comment.author_type)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "human".to_string()),
        author_id: comment.author_id,
        body: comment.body,
        intent: serde_json::to_value(comment.intent)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "progress_update".to_string()),
        mentions: comment.mentions,
        attachment_ids: comment.attachment_ids,
        created_at: comment
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    }
}

fn map_error(err: CommentError) -> StatusCode {
    match err {
        CommentError::TicketNotFound | CommentError::CommentNotFound => StatusCode::NOT_FOUND,
        CommentError::AttachmentNotFound
        | CommentError::InvalidIntent
        | CommentError::Validation(_) => StatusCode::BAD_REQUEST,
        CommentError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn parse_intent(intent: &str) -> Result<CommentIntent, CommentError> {
    intent_from_str(intent).ok_or(CommentError::InvalidIntent)
}

async fn list_comments(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<CommentResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CommentService::new(pool);
    let comments = service
        .list_by_ticket(ticket_id)
        .await
        .map_err(map_error)?;
    Ok(Json(
        comments.into_iter().map(comment_to_response).collect(),
    ))
}

async fn create_comment(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<CreateCommentBody>,
) -> Result<(StatusCode, Json<CommentResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CommentService::new(pool);
    let intent = match body.intent.as_deref() {
        Some(value) => parse_intent(value).map_err(map_error)?,
        None => CommentIntent::ProgressUpdate,
    };
    let attachment_ids = body.attachment_ids.unwrap_or_default();
    let mentions = body.mentions.unwrap_or_default();
    let comment = service
        .create(
            ticket_id,
            AuthorType::Human,
            Some(user.id),
            &body.body,
            intent,
            &attachment_ids,
            &mentions,
        )
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(comment_to_response(comment))))
}
