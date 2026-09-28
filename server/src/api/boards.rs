use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::board::Board;
use crate::services::board_service::{BoardError, BoardService};
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
    Router::new()
        .route("/api/boards", get(list_boards).post(create_board))
        .route(
            "/api/boards/{board_id}",
            get(get_board).patch(update_board),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BoardResponse {
    id: Uuid,
    name: String,
    slug: String,
    created_at: String,
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

fn board_to_response(board: Board) -> BoardResponse {
    BoardResponse {
        id: board.id,
        name: board.name,
        slug: board.slug,
        created_at: board
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    }
}

fn map_error(err: BoardError) -> StatusCode {
    match err {
        BoardError::BoardNotFound => StatusCode::NOT_FOUND,
        BoardError::RepoNotFound => StatusCode::NOT_FOUND,
        BoardError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_boards(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<BoardResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = BoardService::new(pool);
    let boards = service
        .list_boards()
        .await
        .map_err(map_error)?;
    Ok(Json(
        boards.into_iter().map(board_to_response).collect(),
    ))
}

async fn create_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Json(body): Json<CreateBoardBody>,
) -> Result<(StatusCode, Json<BoardResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = BoardService::new(pool);
    let board = service
        .create_board(&body.name)
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(board_to_response(board))))
}

async fn get_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<BoardResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = BoardService::new(pool);
    let board = service
        .get_board(board_id)
        .await
        .map_err(map_error)?;
    Ok(Json(board_to_response(board)))
}

async fn update_board(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(board_id): Path<Uuid>,
    Json(body): Json<UpdateBoardBody>,
) -> Result<Json<BoardResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = BoardService::new(pool);
    let board = service
        .update_board(board_id, body.name.as_deref())
        .await
        .map_err(map_error)?;
    Ok(Json(board_to_response(board)))
}
