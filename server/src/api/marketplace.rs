use crate::api::auth::{pool_from_state, AuthUser};
use crate::api::plugins::{install_response, single_response, stop_unusable_servers, ApiError};
use crate::middleware::admin::AdminUser;
use crate::services::marketplace_service::{
    EntryFilter, EntryOutcome, MarketplaceEntry, MarketplaceJob, MarketplaceService,
    MarketplaceSource,
};
use crate::services::plugin_service::PluginService;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/marketplace/sources",
            get(list_sources).post(add_source),
        )
        .route(
            "/api/marketplace/sources/{source_id}",
            axum::routing::delete(remove_source),
        )
        .route(
            "/api/marketplace/sources/{source_id}/refresh",
            post(refresh_source),
        )
        .route("/api/marketplace/entries", get(list_entries))
        .route("/api/marketplace/install", post(install_entry))
        .route(
            "/api/marketplace/plugins/{plugin_id}/update",
            post(update_plugin),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceResponse {
    id: Uuid,
    name: String,
    kind: String,
    git_url: Option<String>,
    git_ref: Option<String>,
    commit_sha: Option<String>,
    refreshed_at: Option<String>,
    last_error: Option<String>,
    position: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryResponse {
    source_id: Uuid,
    source_name: String,
    name: String,
    description: String,
    version: String,
    kind: String,
    installable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    installed_plugin_id: Option<Uuid>,
    update_available: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddSourceBody {
    name: String,
    git_url: String,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallBody {
    source_id: Uuid,
    entry: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EntriesQuery {
    q: Option<String>,
    source: Option<Uuid>,
    category: Option<String>,
}

async fn list_sources(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<SourceResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let sources = MarketplaceService::new(pool).list_sources().await?;
    Ok(Json(sources.into_iter().map(source_response).collect()))
}

async fn add_source(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<AddSourceBody>,
) -> Result<(StatusCode, Json<SourceResponse>), ApiError> {
    let pool = pool_from_state(&state)?;
    let source = MarketplaceService::new(pool)
        .add_git_source(
            &body.name,
            &body.git_url,
            body.git_ref.as_deref(),
            state.config.plugins.allow_file_git_urls,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(source_response(source))))
}

async fn remove_source(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(source_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let pool = pool_from_state(&state)?;
    MarketplaceService::new(pool)
        .remove_source(source_id, cache_dir(&state))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn refresh_source(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(source_id): Path<Uuid>,
) -> Result<Json<SourceResponse>, ApiError> {
    let pool = pool_from_state(&state)?;
    let timeout = Duration::from_secs(state.config.plugins.git_timeout_secs);
    let source = MarketplaceService::new(pool)
        .refresh_source(
            source_id,
            cache_dir(&state),
            state.config.plugins.allow_file_git_urls,
            timeout,
        )
        .await?;
    Ok(Json(source_response(source)))
}

async fn list_entries(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Query(query): Query<EntriesQuery>,
) -> Result<Json<Vec<EntryResponse>>, ApiError> {
    let pool = pool_from_state(&state)?;
    let filter = EntryFilter {
        query: query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()),
        source_id: query.source,
        category: query
            .category
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty()),
    };
    let entries = MarketplaceService::new(pool)
        .list_entries(cache_dir(&state), &filter)
        .await?;
    Ok(Json(entries.into_iter().map(entry_response).collect()))
}

/// `path` entries answer with the plugin (201); object entries answer with
/// the install job (202) to poll at `/api/plugin-installs/{id}`.
async fn install_entry(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<InstallBody>,
) -> Result<Response, ApiError> {
    let pool = pool_from_state(&state)?;
    let outcome = MarketplaceService::new(pool)
        .install_entry(
            body.source_id,
            cache_dir(&state),
            &body.entry,
            state.config.plugins.allow_file_git_urls,
            &state.skills,
        )
        .await?;
    outcome_response(&state, pool, outcome, StatusCode::CREATED, None).await
}

/// Same answer shapes as install. Updating a `path` plugin replaces its folder
/// in place; an object plugin answers with the update job.
async fn update_plugin(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(plugin_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let pool = pool_from_state(&state)?;
    let outcome = MarketplaceService::new(pool)
        .update_entry(
            plugin_id,
            cache_dir(&state),
            state.config.plugins.allow_file_git_urls,
            &state.skills,
        )
        .await?;
    outcome_response(&state, pool, outcome, StatusCode::OK, Some(plugin_id)).await
}

async fn outcome_response(
    state: &Arc<AppState>,
    pool: &PgPool,
    outcome: EntryOutcome,
    status: StatusCode,
    updated: Option<Uuid>,
) -> Result<Response, ApiError> {
    match outcome {
        EntryOutcome::Plugin(plugin) => {
            if let Some(plugin_id) = updated {
                state.plugin_mcp.stop_plugin(plugin_id).await;
            }
            let Json(body) = single_response(state, pool, *plugin).await?;
            Ok((status, Json(body)).into_response())
        }
        EntryOutcome::Job(job) => {
            let install = install_response(job.install.clone());
            spawn_job(state.clone(), *job);
            Ok((StatusCode::ACCEPTED, install).into_response())
        }
    }
}

/// Runs a git job in the background. A successful update stops the plugin's
/// MCP servers so they restart from the new copy.
fn spawn_job(state: Arc<AppState>, job: MarketplaceJob) {
    tokio::spawn(async move {
        let Some(pool) = state.db.as_ref() else {
            return;
        };
        let timeout = Duration::from_secs(state.config.plugins.git_timeout_secs);
        let update_of = job.update_of;
        let allow_file = state.config.plugins.allow_file_git_urls;
        match MarketplaceService::new(pool)
            .run_job(job, allow_file, timeout, &state.skills)
            .await
        {
            Ok(install) => {
                if let (Some(plugin_id), "succeeded") = (update_of, install.status.as_str()) {
                    state.plugin_mcp.stop_plugin(plugin_id).await;
                }
            }
            Err(err) => tracing::error!(error = %err, "marketplace job could not be recorded"),
        }
        stop_unusable_servers(&PluginService::new(pool), &state).await;
    });
}

fn cache_dir(state: &AppState) -> &FsPath {
    FsPath::new(&state.config.plugins.marketplaces_dir)
}

fn source_response(source: MarketplaceSource) -> SourceResponse {
    SourceResponse {
        id: source.id,
        name: source.name,
        kind: source.kind,
        git_url: source.git_url,
        git_ref: source.git_ref,
        commit_sha: source.commit_sha,
        refreshed_at: source.refreshed_at.map(format_time),
        last_error: source.last_error,
        position: source.position,
    }
}

fn entry_response(entry: MarketplaceEntry) -> EntryResponse {
    EntryResponse {
        source_id: entry.source_id,
        source_name: entry.source_name,
        name: entry.name,
        description: entry.description,
        version: entry.version,
        kind: entry.kind,
        installable: entry.installable,
        reason: entry.reason,
        category: entry.category,
        installed_plugin_id: entry.installed_plugin_id,
        update_available: entry.update_available,
    }
}

fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
