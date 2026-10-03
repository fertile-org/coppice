use crate::middleware::admin::AdminUser;
use crate::services::connector_probe_service::{
    check_connector, list_statuses, CheckError, ConnectorStatusResponse,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tools/connectors", get(list_connectors))
        .route("/api/tools/connectors/{id}/check", post(check))
}

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
) -> Result<Json<Vec<ConnectorStatusResponse>>, StatusCode> {
    list_statuses(&state.connector_probes, &state.config, state.db.as_ref())
        .await
        .map(Json)
        .map_err(|err| {
            tracing::warn!(error = %err, "listing connector statuses failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn check(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<String>,
) -> Result<Json<ConnectorStatusResponse>, StatusCode> {
    check_connector(
        &state.connector_probes,
        &state.config,
        state.db.as_ref(),
        &id,
    )
    .await
    .map(Json)
    .map_err(|err| match err {
        CheckError::NotFound => StatusCode::NOT_FOUND,
        CheckError::ProbeFailed => StatusCode::INTERNAL_SERVER_ERROR,
        CheckError::Db(err) => {
            tracing::warn!(error = %err, "connector check failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })
}
