use crate::api::auth::{pool_from_state, AuthUser};
use crate::services::worktree_admin_service::{
    RemoveResponse, WorktreeAdminError, WorktreeAdminService, WorktreeListResponse,
};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/worktrees", get(list_worktrees))
        .route("/api/worktrees/remove", post(remove_worktrees))
}

struct WorktreeApiError {
    status: StatusCode,
    message: String,
}

impl From<WorktreeAdminError> for WorktreeApiError {
    fn from(error: WorktreeAdminError) -> Self {
        tracing::error!(error = %error, "worktree admin error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Worktree operation failed.".into(),
        }
    }
}

impl From<StatusCode> for WorktreeApiError {
    fn from(status: StatusCode) -> Self {
        Self {
            status,
            message: status.canonical_reason().unwrap_or("error").into(),
        }
    }
}

impl IntoResponse for WorktreeApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveBody {
    folder_names: Vec<String>,
}

async fn list_worktrees(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<WorktreeListResponse>, WorktreeApiError> {
    let pool = pool_from_state(&state)?;
    let list = WorktreeAdminService::new(&state, pool).list().await?;
    Ok(Json(list))
}

async fn remove_worktrees(
    State(state): State<Arc<AppState>>,
    AuthUser { user, .. }: AuthUser,
    Json(body): Json<RemoveBody>,
) -> Result<Json<RemoveResponse>, WorktreeApiError> {
    let pool = pool_from_state(&state)?;
    let result = WorktreeAdminService::new(&state, pool)
        .remove(&body.folder_names, user.id)
        .await?;
    Ok(Json(result))
}
