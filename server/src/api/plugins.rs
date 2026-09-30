use crate::api::auth::{pool_from_state, AuthUser};
use crate::middleware::admin::AdminUser;
use crate::plugins::manifest::{McpServerEntry, SkillEntry};
use crate::services::plugin_service::{PluginDir, PluginError, PluginRow, PluginService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/plugin-dirs", get(list_dirs).post(add_dir))
        .route(
            "/api/plugin-dirs/{dir_id}",
            patch(move_dir).delete(remove_dir),
        )
        .route("/api/plugins", get(list_plugins))
        .route("/api/plugins/rescan", post(rescan))
        .route(
            "/api/plugins/{plugin_id}",
            get(get_plugin).patch(set_enabled),
        )
        .route(
            "/api/agents/{agent_id}/plugins",
            get(get_agent_plugins).put(set_agent_plugins),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginDirResponse {
    id: Uuid,
    path: String,
    position: i32,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginResponse {
    id: Uuid,
    plugin_dir_id: Uuid,
    rel_path: String,
    name: String,
    version: String,
    description: String,
    source: String,
    git_url: Option<String>,
    git_ref: Option<String>,
    git_commit: Option<String>,
    status: String,
    error: Option<String>,
    enabled: bool,
    skills: Vec<SkillEntry>,
    mcp_servers: Vec<McpServerEntry>,
    unsupported: Vec<String>,
}

#[derive(Deserialize)]
struct AddDirBody {
    path: String,
}

#[derive(Deserialize)]
struct MoveDirBody {
    position: i32,
}

#[derive(Deserialize)]
struct SetEnabledBody {
    enabled: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentPluginsBody {
    plugin_ids: Vec<Uuid>,
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<PluginError> for ApiError {
    fn from(err: PluginError) -> Self {
        let status = match &err {
            PluginError::NotFound | PluginError::AgentNotFound => StatusCode::NOT_FOUND,
            PluginError::Validation(_) => StatusCode::BAD_REQUEST,
            PluginError::Conflict(_) => StatusCode::CONFLICT,
            PluginError::Db(_) | PluginError::Io(_) => {
                tracing::error!(error = %err, "plugin request failed");
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

fn dir_response(dir: PluginDir) -> PluginDirResponse {
    PluginDirResponse {
        id: dir.id,
        path: dir.path,
        position: dir.position,
        is_default: dir.is_default,
    }
}

fn plugin_response(plugin: PluginRow) -> PluginResponse {
    let (skills, mcp_servers, unsupported) = plugin
        .manifest
        .map(|m| (m.skills, m.mcp_servers, m.unsupported))
        .unwrap_or_default();
    PluginResponse {
        id: plugin.id,
        plugin_dir_id: plugin.plugin_dir_id,
        rel_path: plugin.rel_path,
        name: plugin.name,
        version: plugin.version,
        description: plugin.description,
        source: plugin.source,
        git_url: plugin.git_url,
        git_ref: plugin.git_ref,
        git_commit: plugin.git_commit,
        status: plugin.status,
        error: plugin.error,
        enabled: plugin.enabled,
        skills,
        mcp_servers,
        unsupported,
    }
}

fn plugins_response(plugins: Vec<PluginRow>) -> Json<Vec<PluginResponse>> {
    Json(plugins.into_iter().map(plugin_response).collect())
}

async fn list_dirs(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<PluginDirResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let dirs = PluginService::new(pool).list_dirs().await?;
    Ok(Json(dirs.into_iter().map(dir_response).collect()))
}

async fn add_dir(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<AddDirBody>,
) -> Result<(StatusCode, Json<PluginDirResponse>), ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    let dir = service.add_dir(&body.path).await?;
    service.refresh_catalog(&state.skills).await?;
    Ok((StatusCode::CREATED, Json(dir_response(dir))))
}

async fn move_dir(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(dir_id): Path<Uuid>,
    Json(body): Json<MoveDirBody>,
) -> Result<Json<Vec<PluginDirResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    let dirs = service.move_dir(dir_id, body.position).await?;
    service.refresh_catalog(&state.skills).await?;
    Ok(Json(dirs.into_iter().map(dir_response).collect()))
}

async fn remove_dir(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(dir_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    service.remove_dir(dir_id).await?;
    service.refresh_catalog(&state.skills).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rescan(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<PluginResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    let plugins = service.rescan().await?;
    service.refresh_catalog(&state.skills).await?;
    Ok(plugins_response(plugins))
}

async fn list_plugins(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<PluginResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    Ok(plugins_response(
        PluginService::new(pool).list_plugins().await?,
    ))
}

async fn get_plugin(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(plugin_id): Path<Uuid>,
) -> Result<Json<PluginResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugin = PluginService::new(pool).get_plugin(plugin_id).await?;
    Ok(Json(plugin_response(plugin)))
}

async fn set_enabled(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(plugin_id): Path<Uuid>,
    Json(body): Json<SetEnabledBody>,
) -> Result<Json<PluginResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    let plugin = service.set_enabled(plugin_id, body.enabled).await?;
    service.refresh_catalog(&state.skills).await?;
    Ok(Json(plugin_response(plugin)))
}

async fn get_agent_plugins(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentPluginsBody>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugin_ids = PluginService::new(pool).agent_plugin_ids(agent_id).await?;
    Ok(Json(AgentPluginsBody { plugin_ids }))
}

async fn set_agent_plugins(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<AgentPluginsBody>,
) -> Result<Json<AgentPluginsBody>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugin_ids = PluginService::new(pool)
        .set_agent_plugins(agent_id, &body.plugin_ids)
        .await?;
    Ok(Json(AgentPluginsBody { plugin_ids }))
}
