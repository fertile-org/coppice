use crate::api::auth::{pool_from_state, AuthUser};
use crate::events::bus::AppEvent;
use crate::services::code_review_service::{
    CodeReviewError, CodeReviewService, SubmitReviewInput, SubmitReviewResponse,
};
use crate::services::comment_service::CommentError;
use crate::services::ticket_service::TicketError;
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
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
        CodeReviewError::Ticket(TicketError::ProjectNotFound) => StatusCode::NOT_FOUND,
        CodeReviewError::Database(_)
        | CodeReviewError::Io(_)
        | CodeReviewError::Ticket(_)
        | CodeReviewError::Comment(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn submit_code_review(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Json(input): Json<SubmitReviewInput>,
) -> Result<(StatusCode, Json<SubmitReviewResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let response = service
        .submit_review(user.id, input)
        .await
        .map_err(map_code_review_error)?;

    state.event_bus.publish(AppEvent::CommentCreated {
        comment_id: response.comment_id,
        ticket_id: response.ticket_id,
        author_type: "human".into(),
    });

    Ok((StatusCode::CREATED, Json(response)))
}
