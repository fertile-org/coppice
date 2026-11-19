use crate::api::auth::pool_from_state;
use crate::middleware::admin::AdminUser;
use crate::services::plugin_build_service::{
    PluginBuild, PluginBuildError, PluginBuildService, PluginBuildUpdate,
};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/plugin-builds", get(list_builds).post(create_build))
        .route(
            "/api/plugin-builds/{id}",
            get(get_build).patch(update_build),
        )
        .route("/api/plugin-builds/{id}/cancel", post(cancel_build))
        .route("/api/plugin-builds/{id}/discard", post(discard_build))
        .route("/api/plugin-builds/{id}/plan-runs", post(start_plan_run))
        .route("/api/plugin-builds/{id}/build-runs", post(start_build_run))
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<PluginBuildError> for ApiError {
    fn from(err: PluginBuildError) -> Self {
        let status = match &err {
            PluginBuildError::NotFound => StatusCode::NOT_FOUND,
            PluginBuildError::Validation(_) => StatusCode::BAD_REQUEST,
            PluginBuildError::Conflict(_) => StatusCode::CONFLICT,
            PluginBuildError::Db(_) | PluginBuildError::Io(_) | PluginBuildError::Worktree(_) => {
                tracing::error!(error = %err, "plugin build request failed");
                return ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred.".into(),
                );
            }
        };
        ApiError(status, err.to_string())
    }
}

impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        ApiError(status, status.canonical_reason().unwrap_or("error").into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildResponse {
    id: Uuid,
    answers: Value,
    plans: Value,
    status: String,
    plugin_name: Option<String>,
    workspace_path: String,
    publish_target: Option<String>,
    plan_questions: Option<Value>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunStartResponse {
    build: BuildResponse,
    run_id: Uuid,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    /// When true, include `discarded` builds. Default: omit them.
    #[serde(default)]
    include_discarded: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBody {
    answers: Option<Value>,
    plan_revision: Option<PlanRevisionBody>,
    plugin_name: Option<String>,
    publish_target: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRevisionBody {
    body: String,
    change_request: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PlanRunBody {
    change_request: Option<String>,
}

fn build_response(build: PluginBuild) -> BuildResponse {
    BuildResponse {
        id: build.id,
        answers: build.answers,
        plans: build.plans,
        status: build.status,
        plugin_name: build.plugin_name,
        workspace_path: build.workspace_path,
        publish_target: build.publish_target,
        plan_questions: build.plan_questions,
        created_at: build
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
        updated_at: build
            .updated_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    }
}

fn service(state: &AppState) -> Result<PluginBuildService<'_>, ApiError> {
    let pool = pool_from_state(state)?;
    Ok(PluginBuildService::new(
        pool,
        state.config.agent.worktrees_path.clone(),
    ))
}

fn connector_is_ready(state: &AppState, connector: &str) -> bool {
    if connector == coppice_connectors::MOCK {
        return true;
    }
    matches!(
        state
            .connector_probes
            .get(connector)
            .map(|cached| cached.readiness),
        Some(coppice_connectors::sign_in::Readiness::Ready)
    )
}

async fn create_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<(StatusCode, Json<BuildResponse>), ApiError> {
    let build = service(&state)?.create().await?;
    Ok((StatusCode::CREATED, Json(build_response(build))))
}

async fn list_builds(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<BuildResponse>>, ApiError> {
    let builds = service(&state)?
        .list(query.include_discarded)
        .await?;
    Ok(Json(builds.into_iter().map(build_response).collect()))
}

async fn get_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let build = service(&state)?.get(id).await?;
    Ok(Json(build_response(build)))
}

async fn update_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<BuildResponse>, ApiError> {
    let update = PluginBuildUpdate {
        answers: body.answers,
        plan_body: body.plan_revision.as_ref().map(|p| p.body.clone()),
        plan_change_request: body.plan_revision.and_then(|p| p.change_request),
        plugin_name: body.plugin_name.map(Some),
        publish_target: body.publish_target.map(Some),
    };
    let build = service(&state)?.update(id, update).await?;
    Ok(Json(build_response(build)))
}

async fn cancel_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let build = service(&state)?.cancel(id).await?;
    Ok(Json(build_response(build)))
}

async fn discard_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let build = service(&state)?.discard(id).await?;
    Ok(Json(build_response(build)))
}

async fn start_plan_run(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PlanRunBody>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let ready = |connector: &str| connector_is_ready(&state, connector);
    let (build, run) = service(&state)?
        .queue_plan_run(id, body.change_request, ready)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            build: build_response(build),
            run_id: run.id,
        }),
    ))
}

async fn start_build_run(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let ready = |connector: &str| connector_is_ready(&state, connector);
    let (build, run) = service(&state)?.queue_build_run(id, ready).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            build: build_response(build),
            run_id: run.id,
        }),
    ))
}
