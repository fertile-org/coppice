use crate::api::auth::pool_from_state;
use crate::middleware::admin::AdminUser;
use crate::services::plugin_build_service::{
    PluginBuild, PluginBuildError, PluginBuildService, PluginBuildUpdate,
};
use crate::services::plugin_publish_service::{PluginPublishService, PublishResult};
use crate::AppState;
use std::path::PathBuf;
use std::time::Duration;
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
        .route("/api/plugin-builds/{id}/accept", post(accept_build))
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
            PluginBuildError::Db(_)
            | PluginBuildError::Io(_)
            | PluginBuildError::Worktree(_)
            | PluginBuildError::Plugin(_) => {
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
    check_errors: Option<Value>,
    format_retries: i32,
    plugin_id: Option<Uuid>,
    test_result: Option<Value>,
    active_run_id: Option<Uuid>,
    last_run_id: Option<Uuid>,
    last_run_status: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    publish: Option<PublishResult>,
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

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AcceptBody {
    enable: Option<bool>,
    publish: Option<bool>,
}

fn build_response(build: PluginBuild) -> BuildResponse {
    build_response_with_publish(build, None)
}

fn build_response_with_publish(build: PluginBuild, publish: Option<PublishResult>) -> BuildResponse {
    BuildResponse {
        id: build.id,
        answers: build.answers,
        plans: build.plans,
        status: build.status,
        plugin_name: build.plugin_name,
        workspace_path: build.workspace_path,
        publish_target: build.publish_target,
        plan_questions: build.plan_questions,
        check_errors: build.check_errors,
        format_retries: build.format_retries,
        plugin_id: build.plugin_id,
        test_result: build.test_result,
        active_run_id: build.active_run_id,
        last_run_id: build.last_run_id,
        last_run_status: build.last_run_status,
        created_at: build
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
        updated_at: build
            .updated_at
            .format(&Rfc3339)
            .unwrap_or_default(),
        publish,
    }
}

async fn enrich_response(
    svc: &PluginBuildService<'_>,
    build: PluginBuild,
) -> Result<BuildResponse, ApiError> {
    Ok(build_response(svc.with_run_meta(build).await?))
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
    let svc = service(&state)?;
    let build = svc.create().await?;
    Ok((StatusCode::CREATED, Json(enrich_response(&svc, build).await?)))
}

async fn list_builds(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<BuildResponse>>, ApiError> {
    let svc = service(&state)?;
    let builds = svc.list(query.include_discarded).await?;
    let mut out = Vec::with_capacity(builds.len());
    for build in builds {
        out.push(enrich_response(&svc, build).await?);
    }
    Ok(Json(out))
}

async fn get_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let svc = service(&state)?;
    let build = svc.get(id).await?;
    Ok(Json(enrich_response(&svc, build).await?))
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
    let svc = service(&state)?;
    let build = svc.update(id, update).await?;
    Ok(Json(enrich_response(&svc, build).await?))
}

async fn cancel_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let svc = service(&state)?;
    let build = svc.cancel(id).await?;
    Ok(Json(enrich_response(&svc, build).await?))
}

async fn discard_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<BuildResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let svc = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = crate::services::plugin_service::PluginService::new(pool);
    let build = svc
        .discard_and_uninstall(
            id,
            &plugins,
            &state.config.plugins.dir,
            &state.plugin_mcp,
            &state.skills,
        )
        .await?;
    Ok(Json(enrich_response(&svc, build).await?))
}

async fn accept_build(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AcceptBody>,
) -> Result<Json<BuildResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let svc = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let plugins = crate::services::plugin_service::PluginService::new(pool);
    let build = svc
        .accept(
            id,
            body.enable.unwrap_or(false),
            &plugins,
            &state.plugin_mcp,
            &state.skills,
        )
        .await?;

    let publish = if body.publish.unwrap_or(false) {
        match (build.plugin_id, build.publish_target.as_deref()) {
            (Some(plugin_id), Some(url)) if !url.is_empty() => {
                let name = build
                    .plugin_name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "plugin".into());
                let publisher = PluginPublishService::new(
                    pool,
                    PathBuf::from(&state.config.agent.worktrees_path),
                    state.config.plugins.allow_file_git_urls,
                    Duration::from_secs(state.config.plugins.git_timeout_secs),
                    state.config.git.push_enabled,
                );
                Some(
                    publisher
                        .publish_after_accept(plugin_id, &name, url, &plugins)
                        .await,
                )
            }
            _ => Some(PublishResult::skipped()),
        }
    } else {
        Some(PublishResult::skipped())
    };

    let build = svc.with_run_meta(build).await?;
    Ok(Json(build_response_with_publish(build, publish)))
}

async fn start_plan_run(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PlanRunBody>,
) -> Result<(StatusCode, Json<RunStartResponse>), ApiError> {
    let ready = |connector: &str| connector_is_ready(&state, connector);
    let svc = service(&state)?;
    let (build, run) = svc
        .queue_plan_run(id, body.change_request, ready)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            build: enrich_response(&svc, build).await?,
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
    let svc = service(&state)?;
    let (build, run) = svc.queue_build_run(id, ready).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(RunStartResponse {
            build: enrich_response(&svc, build).await?,
            run_id: run.id,
        }),
    ))
}
