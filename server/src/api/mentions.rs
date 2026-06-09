use crate::api::auth::{pool_from_state, AuthUser};
use crate::api::tickets::{map_error as map_ticket_error, ticket_to_response, TicketResponse};
use crate::domain::substatus::Substatus;
use crate::services::mention_service::{MentionError, MentionService};
use crate::services::ticket_service::TicketService;
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/mentions/{mention_id}/ignore", post(ignore_mention))
}

fn map_mention_error(err: MentionError) -> StatusCode {
    match err {
        MentionError::MentionNotFound => StatusCode::NOT_FOUND,
        MentionError::Agent(_) | MentionError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn ignore_mention(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(mention_id): Path<Uuid>,
) -> Result<Json<TicketResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let mention_svc = MentionService::new(pool);
    let mention = mention_svc.get(mention_id).await.map_err(map_mention_error)?;
    mention_svc
        .mark_ignored(mention_id)
        .await
        .map_err(map_mention_error)?;

    let ticket_svc = TicketService::new(pool);
    let ticket = ticket_svc.get(mention.ticket_id).await.map_err(map_ticket_error)?;
    let updated = ticket_svc
        .update_status(
            mention.ticket_id,
            ticket.ticket.status,
            Some(Some(Substatus::WaitingForHuman)),
            Some(None),
        )
        .await
        .map_err(map_ticket_error)?;
    Ok(Json(ticket_to_response(updated)))
}
