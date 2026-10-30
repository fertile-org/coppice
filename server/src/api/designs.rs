use crate::api::auth::{pool_from_state, AuthUser};
use crate::services::chat_service::{ChatError, ChatService};
use crate::services::design_service::{
    ChosenOption, DesignError, DesignOption, DesignService, DesignSetSummary,
};
use crate::services::ticket_service::{TicketError, TicketService};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tickets/{ticket_id}/designs", get(list_for_ticket))
        .route(
            "/api/chat/sessions/{session_id}/designs",
            get(list_for_chat),
        )
        .route("/api/designs/{set_id}", get(get_set))
        .route("/api/designs/{set_id}/choose", post(choose))
        .route("/api/designs/{set_id}/files/{*path}", get(get_file))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesignSetResponse {
    id: Uuid,
    title: String,
    run_id: Uuid,
    options: Vec<DesignOption>,
    chosen: Option<ChosenOption>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChooseBody {
    option_id: String,
}

fn artifacts(state: &AppState) -> PathBuf {
    PathBuf::from(&state.config.storage.artifacts_dir)
}

fn to_response(set: DesignSetSummary) -> DesignSetResponse {
    DesignSetResponse {
        id: set.id,
        title: set.title,
        run_id: set.run_id,
        options: set.options,
        chosen: set.chosen,
    }
}

fn map_design(err: DesignError) -> StatusCode {
    match err {
        DesignError::Invalid(_) => StatusCode::BAD_REQUEST,
        DesignError::NotFound => StatusCode::NOT_FOUND,
        DesignError::Denied => StatusCode::FORBIDDEN,
        DesignError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn visible(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    ticket_id: Option<Uuid>,
    chat_session_id: Option<Uuid>,
) -> Result<(), StatusCode> {
    if let Some(chat_session_id) = chat_session_id {
        return ChatService::new(pool)
            .get_session(chat_session_id, user_id)
            .await
            .map(|_| ())
            .map_err(|err| match err {
                ChatError::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            });
    }
    if let Some(ticket_id) = ticket_id {
        return TicketService::new(pool)
            .get(ticket_id)
            .await
            .map(|_| ())
            .map_err(|err| match err {
                TicketError::TicketNotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            });
    }
    Err(StatusCode::NOT_FOUND)
}

async fn list_for_ticket(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<DesignSetResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    visible(pool, user.id, Some(ticket_id), None).await?;
    let sets = DesignService::new(pool, artifacts(&state))
        .list_for_ticket(ticket_id)
        .await
        .map_err(map_design)?;
    Ok(Json(sets.into_iter().map(to_response).collect()))
}

async fn list_for_chat(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Vec<DesignSetResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    visible(pool, user.id, None, Some(session_id)).await?;
    let sets = DesignService::new(pool, artifacts(&state))
        .list_for_chat(session_id)
        .await
        .map_err(map_design)?;
    Ok(Json(sets.into_iter().map(to_response).collect()))
}

async fn get_set(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(set_id): Path<Uuid>,
) -> Result<Json<DesignSetResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let record = DesignService::new(pool, artifacts(&state))
        .get(set_id)
        .await
        .map_err(map_design)?;
    visible(
        pool,
        user.id,
        record.ticket_id,
        record.chat_session_id,
    )
    .await?;
    Ok(Json(to_response(record.summary)))
}

async fn choose(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path(set_id): Path<Uuid>,
    Json(body): Json<ChooseBody>,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    DesignService::new(pool, artifacts(&state))
        .choose(set_id, &body.option_id, user.id)
        .await
        .map_err(map_design)?;
    Ok(StatusCode::NO_CONTENT)
}

fn media_type(path: &str) -> &'static str {
    match FsPath::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Path((set_id, path)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let pool = pool_from_state(&state)?;
    let svc = DesignService::new(pool, artifacts(&state));
    let record = svc.get(set_id).await.map_err(map_design)?;
    visible(
        pool,
        user.id,
        record.ticket_id,
        record.chat_session_id,
    )
    .await?;
    let bytes = svc.read_file(set_id, &path).await.map_err(map_design)?;
    Ok(([(header::CONTENT_TYPE, media_type(&path))], Body::from(bytes)))
}
