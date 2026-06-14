use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::comment::{intent_from_str, AuthorType, Comment, CommentIntent};
use crate::events::bus::AppEvent;
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::mention_service::{MentionError, MentionService};
use crate::services::run_service::{RunService, StartRunOptions};
use crate::services::ticket_service::{TicketError, TicketService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
struct AttachmentSummary {
    id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
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
    attachments: Vec<AttachmentSummary>,
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

fn attachment_to_summary(attachment: &crate::domain::attachment::Attachment) -> AttachmentSummary {
    AttachmentSummary {
        id: attachment.id,
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
    }
}

fn comment_to_response(
    comment: Comment,
    attachments_by_id: &HashMap<Uuid, crate::domain::attachment::Attachment>,
) -> CommentResponse {
    let attachments = comment
        .attachment_ids
        .iter()
        .filter_map(|id| attachments_by_id.get(id))
        .map(attachment_to_summary)
        .collect();

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
        attachments,
        created_at: comment
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    }
}

async fn attachments_for_comments(
    service: &CommentService<'_>,
    comments: &[Comment],
) -> Result<HashMap<Uuid, crate::domain::attachment::Attachment>, CommentError> {
    let mut ids: Vec<Uuid> = comments
        .iter()
        .flat_map(|comment| comment.attachment_ids.clone())
        .collect();
    ids.sort_unstable();
    ids.dedup();

    let attachments = service.list_attachments_by_ids(&ids).await?;
    Ok(attachments
        .into_iter()
        .map(|attachment| (attachment.id, attachment))
        .collect())
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

fn map_ticket_error(err: TicketError) -> StatusCode {
    match err {
        TicketError::TicketNotFound | TicketError::ProjectNotFound => StatusCode::NOT_FOUND,
        TicketError::InvalidStatus
        | TicketError::InvalidSubstatus
        | TicketError::InvalidPriority
        | TicketError::Validation(_) => StatusCode::BAD_REQUEST,
        TicketError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn map_mention_error(err: MentionError) -> StatusCode {
    match err {
        MentionError::MentionNotFound => StatusCode::NOT_FOUND,
        MentionError::Agent(_) | MentionError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
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
    let attachments_by_id = attachments_for_comments(&service, &comments)
        .await
        .map_err(map_error)?;
    Ok(Json(
        comments
            .into_iter()
            .map(|comment| comment_to_response(comment, &attachments_by_id))
            .collect(),
    ))
}

async fn create_comment(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<CreateCommentBody>,
) -> Result<(StatusCode, Json<CommentResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .map_err(map_ticket_error)?;

    let agents = AgentService::new(pool)
        .list_agents()
        .await
        .map_err(|err: AgentError| match err {
            AgentError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    let agent_keys = build_agent_keys(&agents);
    let key_refs: Vec<&str> = agent_keys.iter().map(String::as_str).collect();
    let mut parsed_mentions = MentionService::parse_mention_keys(&body.body, &key_refs);
    if parsed_mentions.is_empty() {
        parsed_mentions = body.mentions.unwrap_or_default();
    }

    let service = CommentService::new(pool);
    let intent = match body.intent.as_deref() {
        Some(value) => parse_intent(value).map_err(map_error)?,
        None => CommentIntent::ProgressUpdate,
    };
    let attachment_ids = body.attachment_ids.unwrap_or_default();
    let comment = service
        .create(
            ticket_id,
            AuthorType::Human,
            Some(user.id),
            &body.body,
            intent,
            &attachment_ids,
            &parsed_mentions,
        )
        .await
        .map_err(map_error)?;

    if !parsed_mentions.is_empty() {
        let mention_svc = MentionService::new(pool);
        let mentions = mention_svc
            .create_mentions(
                ticket_id,
                comment.id,
                &parsed_mentions,
                None,
                ticket.ticket.project_id,
            )
            .await
            .map_err(map_mention_error)?;

        if state.config.workflow.auto_start_runs && ticket.ticket.repo_id.is_some() {
            let run_svc = RunService::new(pool);
            for mention in &mentions {
                let _ = run_svc
                    .start_run_for_agent(
                        ticket_id,
                        mention.mentioned_agent_id,
                        "respond_to_mention",
                        StartRunOptions::default(),
                    )
                    .await;
            }
        }

        for mention in &mentions {
            state.event_bus.publish(AppEvent::AgentMentioned {
                mention_id: mention.id,
                ticket_id,
                comment_id: comment.id,
                mentioned_agent_id: mention.mentioned_agent_id,
            });
        }
    }

    let attachments_by_id = attachments_for_comments(&service, std::slice::from_ref(&comment))
        .await
        .map_err(map_error)?;
    Ok((
        StatusCode::CREATED,
        Json(comment_to_response(comment, &attachments_by_id)),
    ))
}

fn build_agent_keys(agents: &[crate::domain::agent::Agent]) -> Vec<String> {
    use crate::domain::slug::slugify;

    let mut keys = Vec::new();
    for agent in agents {
        if !agent.enabled {
            continue;
        }
        if let Some(ref preset) = agent.preset_source {
            if !keys.iter().any(|k| k == preset) {
                keys.push(preset.clone());
            }
        }
        let slug = slugify(&agent.name);
        if !keys.iter().any(|k| k == &slug) {
            keys.push(slug);
        }
    }
    keys
}
