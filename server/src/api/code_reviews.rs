use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::substatus::TicketStatus;
use crate::events::bus::AppEvent;
use crate::services::code_review_service::{
    CodeReviewError, CodeReviewService, SubmitReviewInput, SubmitReviewResponse,
};
use crate::services::comment_service::CommentError;
use crate::services::run_service::RunService;
use crate::services::ticket_service::{TicketError, TicketService, TicketWithDisplay};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/code-reviews/submit", post(submit_code_review))
}

pub(crate) fn map_code_review_error(err: CodeReviewError) -> StatusCode {
    match err {
        CodeReviewError::RepoNotFound | CodeReviewError::TicketNotFound => StatusCode::NOT_FOUND,
        CodeReviewError::Ticket(crate::services::ticket_service::TicketError::TicketNotFound) => {
            StatusCode::NOT_FOUND
        }
        CodeReviewError::RepoNotReady
        | CodeReviewError::InvalidWorktreePath
        | CodeReviewError::InvalidFilePath
        | CodeReviewError::InvalidBranchName
        | CodeReviewError::TicketRepoMismatch
        | CodeReviewError::Validation(_)
        | CodeReviewError::Git(_) => StatusCode::BAD_REQUEST,
        CodeReviewError::PatchTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        CodeReviewError::Ticket(TicketError::Validation(_)) => StatusCode::BAD_REQUEST,
        CodeReviewError::Comment(CommentError::Validation(_)) => StatusCode::BAD_REQUEST,
        CodeReviewError::Ticket(TicketError::BoardNotFound) => StatusCode::NOT_FOUND,
        CodeReviewError::Database(_)
        | CodeReviewError::Io(_)
        | CodeReviewError::Ticket(_)
        | CodeReviewError::Comment(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

enum SubmitReviewError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for SubmitReviewError {
    fn into_response(self) -> Response {
        match self {
            SubmitReviewError::Status(code) => code.into_response(),
            SubmitReviewError::Message(code, error) => {
                (code, Json(serde_json::json!({ "error": error }))).into_response()
            }
        }
    }
}

fn map_submit_review_error(err: CodeReviewError) -> SubmitReviewError {
    match err {
        CodeReviewError::Validation(message) => {
            SubmitReviewError::Message(StatusCode::BAD_REQUEST, message)
        }
        CodeReviewError::Ticket(TicketError::Validation(message)) => {
            SubmitReviewError::Message(StatusCode::BAD_REQUEST, message)
        }
        other => SubmitReviewError::Status(map_code_review_error(other)),
    }
}

fn should_start_run(state: &AppState, ticket: &TicketWithDisplay, action: &str) -> bool {
    if !state.config.workflow.auto_start_runs
        || ticket.ticket.assignee_agent_id.is_none()
        || ticket.ticket.repo_id.is_none()
        || ticket.ticket.status == TicketStatus::Done
    {
        return false;
    }
    match action {
        "move_to_in_progress" => true,
        // A work run in a review column would leave the card there while the engineer works.
        "reassign_engineer" => matches!(
            ticket.ticket.status,
            TicketStatus::InProgress | TicketStatus::Ready | TicketStatus::Backlog
        ),
        _ => false,
    }
}

async fn publish_workflow_follow_up(
    state: &AppState,
    pool: &sqlx::PgPool,
    response: &SubmitReviewResponse,
) {
    let Some(action) = response.workflow_action.as_deref() else {
        return;
    };
    let ticket_id = response.ticket_id;
    let Ok(mut ticket) = TicketService::new(pool).get(ticket_id).await else {
        tracing::warn!(%ticket_id, "ticket refresh after code review failed");
        return;
    };
    if should_start_run(state, &ticket, action) {
        if let Ok(run) = RunService::new(pool).start_run(ticket_id).await {
            if let Ok(updated) = TicketService::new(pool).get(ticket_id).await {
                ticket = updated;
            }
            crate::events::publish_ticket_updated(&state.event_bus, &ticket);
            crate::events::publish_run_active(&state.event_bus, &run);
            return;
        }
    }
    crate::events::publish_ticket_updated(&state.event_bus, &ticket);
}

async fn submit_code_review(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Json(input): Json<SubmitReviewInput>,
) -> Result<(StatusCode, Json<SubmitReviewResponse>), SubmitReviewError> {
    let pool = pool_from_state(&state).map_err(SubmitReviewError::Status)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let response = service
        .submit_review(user.id, input)
        .await
        .map_err(map_submit_review_error)?;

    state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: response.comment_id,
        ticket_id: response.ticket_id,
        author_type: "human".into(),
    });

    publish_workflow_follow_up(&state, pool, &response).await;

    Ok((StatusCode::CREATED, Json(response)))
}
