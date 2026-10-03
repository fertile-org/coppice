use crate::middleware::admin::AdminUser;
use crate::services::connector_check_service::{
    CheckDetail, CheckError as ConnectorCheckError, ConnectorCheckService,
};
use crate::services::connector_probe_service::{
    check_connector, diagnosable, list_statuses, CheckError, ConnectorStatusResponse,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tools/connectors", get(list_connectors))
        .route("/api/tools/connectors/{id}/check", post(check))
        .route("/api/tools/connectors/{id}/test", post(test_connection))
        .route("/api/tools/connector-checks/{id}", get(check_detail))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestConnectionRequest {
    agent_id: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestConnectionResponse {
    check_id: Uuid,
    run_id: Uuid,
}

async fn test_connection(
    State(state): State<Arc<AppState>>,
    AdminUser(admin): AdminUser,
    Path(id): Path<String>,
    Json(body): Json<TestConnectionRequest>,
) -> Result<Json<TestConnectionResponse>, StatusCode> {
    if diagnosable(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let pool = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let (check_id, run_id) = ConnectorCheckService::new(pool)
        .start(&state.config, &id, body.agent_id, admin.user.id)
        .await
        .map_err(check_error_status)?;
    Ok(Json(TestConnectionResponse { check_id, run_id }))
}

async fn check_detail(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<CheckDetail>, StatusCode> {
    let pool = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    ConnectorCheckService::new(pool)
        .get(id)
        .await
        .map(Json)
        .map_err(check_error_status)
}

fn check_error_status(err: ConnectorCheckError) -> StatusCode {
    match err {
        ConnectorCheckError::ActiveCheck => StatusCode::CONFLICT,
        ConnectorCheckError::WrongConnector | ConnectorCheckError::Disabled => {
            StatusCode::BAD_REQUEST
        }
        ConnectorCheckError::UnknownConnector
        | ConnectorCheckError::AgentNotFound
        | ConnectorCheckError::NotFound => StatusCode::NOT_FOUND,
        other => {
            tracing::warn!(error = %other, "connector check request failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
