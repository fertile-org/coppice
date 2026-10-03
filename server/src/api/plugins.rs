use crate::api::auth::{pool_from_state, AuthUser};
use crate::mcp::proxy::{McpServerPool, ServerHealth, ServerKey};
use crate::middleware::admin::AdminUser;
use crate::plugins::capability::{McpServerEntry, McpServerTransport};
use crate::plugins::git_install;
use crate::plugins::manifest::SkillEntry;
use crate::plugins::placeholders::placeholder_keys;
use crate::services::plugin_service::{
    PluginDir, PluginError, PluginInstall, PluginRow, PluginService, ServerTestOutcome,
    ServerTestResult,
};
use crate::services::plugin_settings_service::{PluginSettingsError, PluginSettingsService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
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
        .route("/api/plugins/install", post(install))
        .route(
            "/api/plugins/{plugin_id}",
            get(get_plugin).patch(set_enabled),
        )
        .route("/api/plugins/{plugin_id}/update", post(update))
        .route("/api/plugins/{plugin_id}/settings", put(set_settings))
        .route("/api/plugins/{plugin_id}/test", post(test_plugin))
        .route("/api/plugin-installs/{install_id}", get(get_install))
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
    mcp_servers: Vec<McpServerResponse>,
    unsupported: Vec<String>,
    settings: Vec<PluginSettingResponse>,
}

/// Commands, env, URLs, and headers can carry secrets and are never exposed.
#[derive(Serialize)]
struct McpServerResponse {
    name: String,
    kind: String,
    health: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginTestResponse {
    servers: Vec<ServerTestResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerTestResponse {
    name: String,
    kind: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    tools: Vec<TestedToolResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TestedToolResponse {
    name: String,
    exposed_name: String,
    description: String,
    read_only: bool,
}

/// Setting values are write-only.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingResponse {
    key: String,
    configured: bool,
}

#[derive(Deserialize)]
struct SettingsBody {
    values: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstallResponse {
    id: Uuid,
    plugin_dir_id: Uuid,
    kind: String,
    git_url: String,
    git_ref: Option<String>,
    plugin_id: Option<Uuid>,
    status: String,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallBody {
    git_url: String,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    plugin_dir_id: Uuid,
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
            PluginError::NotFound | PluginError::AgentNotFound | PluginError::InstallNotFound => {
                StatusCode::NOT_FOUND
            }
            PluginError::Validation(_) => StatusCode::BAD_REQUEST,
            PluginError::Conflict(_) => StatusCode::CONFLICT,
            PluginError::Db(_) | PluginError::Io(_) | PluginError::Settings(_) => {
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

impl From<PluginSettingsError> for ApiError {
    fn from(err: PluginSettingsError) -> Self {
        match err {
            PluginSettingsError::UnknownKey(_) => {
                ApiError(StatusCode::BAD_REQUEST, err.to_string())
            }
            PluginSettingsError::Secret(_) | PluginSettingsError::Db(_) => {
                tracing::error!(error = %err, "plugin settings request failed");
                ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred.".into(),
                )
            }
        }
    }
}

impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        ApiError(status, status.canonical_reason().unwrap_or("error").into())
    }
}

/// The mutation has already committed, so a failed refresh is logged rather
/// than reported; the next refresh picks the change up.
async fn refresh_skills(service: &PluginService<'_>, state: &AppState) {
    if let Err(err) = service.refresh_catalog(&state.skills).await {
        tracing::error!(error = %err, "failed to refresh plugin skill catalog");
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

/// A plugin without a manifest has no setting keys.
fn allowed_setting_keys(plugin: &PluginRow) -> BTreeSet<String> {
    plugin
        .manifest
        .as_ref()
        .map(|m| placeholder_keys(&m.mcp_servers))
        .unwrap_or_default()
}

fn server_health(mcp: &McpServerPool, plugin_id: Uuid, entry: &McpServerEntry) -> &'static str {
    if let McpServerTransport::Unsupported { .. } = entry.transport {
        return ServerHealth::Stopped.as_str();
    }
    mcp.health(&ServerKey {
        plugin_id,
        server: entry.name.clone(),
    })
    .as_str()
}

fn plugin_response(
    plugin: PluginRow,
    configured: &BTreeSet<String>,
    mcp: &McpServerPool,
) -> PluginResponse {
    let settings = allowed_setting_keys(&plugin)
        .into_iter()
        .map(|key| PluginSettingResponse {
            configured: configured.contains(&key),
            key,
        })
        .collect();
    let (skills, mcp_servers, unsupported) = plugin
        .manifest
        .map(|m| {
            let mcp_servers = m
                .mcp_servers
                .iter()
                .map(|s| McpServerResponse {
                    name: s.name.clone(),
                    kind: s.api_kind().to_string(),
                    health: server_health(mcp, plugin.id, s),
                })
                .collect();
            let unsupported = m.unsupported.into_iter().map(|u| u.key).collect();
            (m.skills, mcp_servers, unsupported)
        })
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
        settings,
    }
}

async fn single_response(
    state: &AppState,
    pool: &PgPool,
    plugin: PluginRow,
) -> Result<Json<PluginResponse>, ApiError> {
    let configured = PluginSettingsService::new(pool, &state.secret_store)
        .configured_keys(plugin.id)
        .await?;
    Ok(Json(plugin_response(
        plugin,
        &configured,
        &state.plugin_mcp,
    )))
}

async fn plugins_response(
    state: &AppState,
    pool: &PgPool,
    plugins: Vec<PluginRow>,
) -> Result<Json<Vec<PluginResponse>>, ApiError> {
    let configured = PluginSettingsService::new(pool, &state.secret_store)
        .configured_keys_by_plugin()
        .await?;
    let none = BTreeSet::new();
    Ok(Json(
        plugins
            .into_iter()
            .map(|p| {
                let keys = configured.get(&p.id).unwrap_or(&none);
                plugin_response(p, keys, &state.plugin_mcp)
            })
            .collect(),
    ))
}

fn install_response(install: PluginInstall) -> Json<PluginInstallResponse> {
    Json(PluginInstallResponse {
        id: install.id,
        plugin_dir_id: install.plugin_dir_id,
        kind: install.kind,
        git_url: install.git_url,
        git_ref: install.git_ref,
        plugin_id: install.plugin_id,
        status: install.status,
        error: install.error,
    })
}

/// Runs `job` in the background, then records its outcome; nothing borrowed
/// from the request may be captured. A successful update of `updated_plugin`
/// stops its MCP servers so they restart from the new checkout.
fn spawn_git_job<F>(
    state: Arc<AppState>,
    install_id: Uuid,
    dest_rel: String,
    updated_plugin: Option<Uuid>,
    job: F,
) where
    F: Future<Output = Result<String, String>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = job.await;
        let succeeded = result.is_ok();
        let Some(pool) = state.db.as_ref() else {
            return;
        };
        PluginService::new(pool)
            .complete_git_job(install_id, result, &dest_rel, &state.skills)
            .await;
        if let (true, Some(plugin_id)) = (succeeded, updated_plugin) {
            state.plugin_mcp.stop_plugin(plugin_id).await;
        }
    });
}

fn folder_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn install(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<InstallBody>,
) -> Result<(StatusCode, Json<PluginInstallResponse>), ApiError> {
    let pool = pool_from_state(&state)?;
    let cfg = &state.config.plugins;
    let (install, dest) = PluginService::new(pool)
        .start_install(
            &body.git_url,
            body.git_ref.as_deref(),
            body.plugin_dir_id,
            cfg,
        )
        .await?;
    let (url, git_ref) = (install.git_url.clone(), install.git_ref.clone());
    let (allow_file, timeout) = (
        cfg.allow_file_git_urls,
        Duration::from_secs(cfg.git_timeout_secs),
    );
    let dest_rel = folder_name(&dest);
    let job = async move {
        git_install::clone(&url, git_ref.as_deref(), &dest, allow_file, timeout).await
    };
    spawn_git_job(state.clone(), install.id, dest_rel, None, job);
    Ok((StatusCode::ACCEPTED, install_response(install)))
}

async fn update(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(plugin_id): Path<Uuid>,
) -> Result<(StatusCode, Json<PluginInstallResponse>), ApiError> {
    let pool = pool_from_state(&state)?;
    let cfg = &state.config.plugins;
    let (install, root, rel_path) = PluginService::new(pool).start_update(plugin_id).await?;
    let git_ref = install.git_ref.clone();
    let (allow_file, timeout) = (
        cfg.allow_file_git_urls,
        Duration::from_secs(cfg.git_timeout_secs),
    );
    let job =
        async move { git_install::update(&root, git_ref.as_deref(), allow_file, timeout).await };
    spawn_git_job(state.clone(), install.id, rel_path, Some(plugin_id), job);
    Ok((StatusCode::ACCEPTED, install_response(install)))
}

async fn get_install(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(install_id): Path<Uuid>,
) -> Result<Json<PluginInstallResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let install = PluginService::new(pool).get_install(install_id).await?;
    Ok(install_response(install))
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
    refresh_skills(&service, &state).await;
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
    refresh_skills(&service, &state).await;
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
    refresh_skills(&service, &state).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn rescan(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<Json<Vec<PluginResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let service = PluginService::new(pool);
    let plugins = service.rescan().await?;
    refresh_skills(&service, &state).await;
    plugins_response(&state, pool, plugins).await
}

async fn list_plugins(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<PluginResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugins = PluginService::new(pool).list_plugins().await?;
    plugins_response(&state, pool, plugins).await
}

async fn get_plugin(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(plugin_id): Path<Uuid>,
) -> Result<Json<PluginResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugin = PluginService::new(pool).get_plugin(plugin_id).await?;
    single_response(&state, pool, plugin).await
}

async fn set_settings(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(plugin_id): Path<Uuid>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<PluginResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let plugin = PluginService::new(pool).get_plugin(plugin_id).await?;
    PluginSettingsService::new(pool, &state.secret_store)
        .set(plugin_id, &allowed_setting_keys(&plugin), body.values)
        .await?;
    single_response(&state, pool, plugin).await
}

async fn test_plugin(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(plugin_id): Path<Uuid>,
) -> Result<Json<PluginTestResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let results = PluginService::new(pool)
        .test_servers(plugin_id, &state.secret_store, &state.plugin_mcp)
        .await?;
    Ok(Json(PluginTestResponse {
        servers: results.into_iter().map(server_test_response).collect(),
    }))
}

fn server_test_response(result: ServerTestResult) -> ServerTestResponse {
    let (status, error, tools) = match result.outcome {
        ServerTestOutcome::Ok(tools) => ("ok", None, tools),
        ServerTestOutcome::Error(error) => ("error", Some(error), Vec::new()),
        ServerTestOutcome::Unsupported { error } => ("unsupported", error, Vec::new()),
    };
    ServerTestResponse {
        name: result.name,
        kind: result.kind,
        status,
        error,
        tools: tools
            .into_iter()
            .map(|t| TestedToolResponse {
                name: t.name,
                exposed_name: t.exposed_name,
                description: t.description,
                read_only: t.read_only,
            })
            .collect(),
    }
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
    if !plugin.enabled {
        state.skills.remove_plugin(plugin.id).await;
        state.plugin_mcp.stop_plugin(plugin.id).await;
    }
    refresh_skills(&service, &state).await;
    single_response(&state, pool, plugin).await
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
