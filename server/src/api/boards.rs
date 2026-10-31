use crate::api::auth::{pool_from_state, AuthUser};
use crate::copy::review::{
    BOARD_NAME_CONFLICT, BOARD_NAME_INVALID, BOARD_NAME_LABEL, BOARD_SETTINGS_LABEL,
    COUNT_IN_REVIEW_LABEL, REVIEW_CAP_HINT, REVIEW_CAP_INVALID, REVIEW_CAP_LABEL,
};
use crate::domain::board::Board;
use crate::services::board_service::{BoardError, BoardService};
use crate::services::review_capacity::{
    review_cap_limit, ReviewCapacityError, ReviewCapacityService, ReviewPolicy,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/boards", get(list_boards).post(create_board))
        .route("/api/boards/{board_id}", get(get_board).patch(update_board))
        .route(
            "/api/boards/{board_id}/review-capacity",
            axum::routing::put(put_review_capacity),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardResponse {
    id: Uuid,
    name: String,
    slug: String,
    created_at: String,
    review_cap: Option<i32>,
    count_in_review: bool,
    waiting_on_review: Option<String>,
    board_settings_label: &'static str,
    name_label: &'static str,
    review_cap_label: &'static str,
    review_cap_hint: &'static str,
    count_in_review_label: &'static str,
    review_cap_invalid: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewCapacityBody {
    review_cap: Option<i32>,
    count_in_review: bool,
}

#[derive(Serialize)]
struct ApiMessageResponse {
    message: String,
}

enum BoardApiError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for BoardApiError {
    fn into_response(self) -> Response {
        match self {
            BoardApiError::Status(code) => code.into_response(),
            BoardApiError::Message(code, message) => {
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBoardBody {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBoardBody {
    name: Option<String>,
}

fn board_to_response(board: Board, waiting_on_review: Option<String>) -> BoardResponse {
    BoardResponse {
        id: board.id,
        name: board.name,
        slug: board.slug,
        created_at: board.created_at.format(&Rfc3339).unwrap_or_default(),
        review_cap: board.review_cap,
        count_in_review: board.count_in_review,
        waiting_on_review,
        board_settings_label: BOARD_SETTINGS_LABEL,
        name_label: BOARD_NAME_LABEL,
        review_cap_label: REVIEW_CAP_LABEL,
        review_cap_hint: REVIEW_CAP_HINT,
        count_in_review_label: COUNT_IN_REVIEW_LABEL,
        review_cap_invalid: REVIEW_CAP_INVALID,
    }
}

async fn response_for(pool: &sqlx::PgPool, board: Board) -> Result<BoardResponse, BoardApiError> {
    let waiting = ReviewCapacityService::new(pool)
        .waiting_line(board.id)
        .await
        .map_err(map_capacity_error)?;
    Ok(board_to_response(board, waiting))
}

fn map_error(err: BoardError) -> StatusCode {
    match err {
        BoardError::BoardNotFound => StatusCode::NOT_FOUND,
        BoardError::RepoNotFound => StatusCode::NOT_FOUND,
        BoardError::InvalidName => StatusCode::BAD_REQUEST,
        BoardError::NameConflict => StatusCode::CONFLICT,
        BoardError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_boards(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<BoardResponse>>, BoardApiError> {
    let pool = pool_from_state(&state).map_err(BoardApiError::Status)?;
    let service = BoardService::new(pool);
    let boards = service.list_boards().await.map_err(map_board_error)?;
    let mut responses = Vec::with_capacity(boards.len());
    for board in boards {
        responses.push(response_for(pool, board).await?);
    }
    Ok(Json(responses))
}

async fn create_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Json(body): Json<CreateBoardBody>,
) -> Result<(StatusCode, Json<BoardResponse>), BoardApiError> {
    let pool = pool_from_state(&state).map_err(BoardApiError::Status)?;
    let service = BoardService::new(pool);
    let board = service
        .create_board(&body.name)
        .await
        .map_err(map_board_error)?;
    Ok((StatusCode::CREATED, Json(response_for(pool, board).await?)))
}

async fn get_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<BoardResponse>, BoardApiError> {
    let pool = pool_from_state(&state).map_err(BoardApiError::Status)?;
    let service = BoardService::new(pool);
    let board = service.get_board(board_id).await.map_err(map_board_error)?;
    Ok(Json(response_for(pool, board).await?))
}

async fn update_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
    Json(body): Json<UpdateBoardBody>,
) -> Result<Json<BoardResponse>, BoardApiError> {
    let pool = pool_from_state(&state).map_err(BoardApiError::Status)?;
    let service = BoardService::new(pool);
    let board = service
        .update_board(board_id, body.name.as_deref())
        .await
        .map_err(map_board_error)?;
    Ok(Json(response_for(pool, board).await?))
}

async fn put_review_capacity(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
    Json(body): Json<ReviewCapacityBody>,
) -> Result<Json<BoardResponse>, BoardApiError> {
    let review_cap = review_cap_limit(body.review_cap, REVIEW_CAP_INVALID)
        .map_err(|message| BoardApiError::Message(StatusCode::BAD_REQUEST, message))?;
    let pool = pool_from_state(&state).map_err(BoardApiError::Status)?;
    let capacity = ReviewCapacityService::new(pool);
    capacity
        .set_policy(
            board_id,
            ReviewPolicy {
                cap: review_cap,
                count_in_review: body.count_in_review,
            },
        )
        .await
        .map_err(map_capacity_error)?;
    let board = BoardService::new(pool)
        .get_board(board_id)
        .await
        .map_err(map_board_error)?;
    Ok(Json(response_for(pool, board).await?))
}

fn map_board_error(err: BoardError) -> BoardApiError {
    match err {
        BoardError::InvalidName => {
            BoardApiError::Message(StatusCode::BAD_REQUEST, BOARD_NAME_INVALID.to_string())
        }
        BoardError::NameConflict => {
            BoardApiError::Message(StatusCode::CONFLICT, BOARD_NAME_CONFLICT.to_string())
        }
        other => BoardApiError::Status(map_error(other)),
    }
}

fn map_capacity_error(err: ReviewCapacityError) -> BoardApiError {
    match err {
        ReviewCapacityError::Message(message) if message == REVIEW_CAP_INVALID => {
            BoardApiError::Message(StatusCode::BAD_REQUEST, message)
        }
        ReviewCapacityError::Message(message) if message == "board not found" => {
            BoardApiError::Status(StatusCode::NOT_FOUND)
        }
        ReviewCapacityError::Message(message) => {
            tracing::warn!(%message, "review capacity request failed");
            BoardApiError::Status(StatusCode::BAD_REQUEST)
        }
        ReviewCapacityError::Database(err) => {
            tracing::warn!(error = %err, "review capacity request failed");
            BoardApiError::Status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
