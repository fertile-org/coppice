use crate::api::auth::{pool_from_state, AuthUser};
use crate::connectors_runtime::ConfigFileError;
use crate::domain::agent::Agent;
use crate::middleware::admin::AdminUser;
use crate::providers::READ_ONLY_CAPABLE_CONNECTORS;
use crate::services::workspace_settings_service::{
    WorkspaceSettingsError, WorkspaceSettingsService,
};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/settings/knowledge",
            get(get_knowledge_settings).put(put_knowledge_settings),
        )
        .route(
            "/api/settings/config",
            get(get_config_file).put(put_config_file),
        )
        .route("/api/settings/config/backup", get(get_config_backup))
}

struct SettingsApiError {
    status: StatusCode,
    message: String,
}

impl From<WorkspaceSettingsError> for SettingsApiError {
    fn from(error: WorkspaceSettingsError) -> Self {
        match error {
            WorkspaceSettingsError::AgentNotFound
            | WorkspaceSettingsError::ConnectorNotReadOnly(_) => Self {
                status: StatusCode::BAD_REQUEST,
                message: error.to_string(),
            },
            WorkspaceSettingsError::Database(database_error) => {
                tracing::error!(error = %database_error, "workspace settings database error");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Settings operation failed.".into(),
                }
            }
        }
    }
}

impl IntoResponse for SettingsApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompactionAgentResponse {
    id: Uuid,
    name: String,
    enabled: bool,
    connector: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeSettingsResponse {
    compaction_agent_id: Option<Uuid>,
    compaction_agent: Option<CompactionAgentResponse>,
    read_only_connectors: &'static [&'static str],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeSettingsBody {
    compaction_agent_id: Option<Uuid>,
}

fn settings_response(agent: Option<Agent>) -> KnowledgeSettingsResponse {
    KnowledgeSettingsResponse {
        compaction_agent_id: agent.as_ref().map(|agent| agent.id),
        compaction_agent: agent.map(|agent| CompactionAgentResponse {
            id: agent.id,
            name: agent.name,
            enabled: agent.enabled,
            connector: agent.connector,
        }),
        read_only_connectors: READ_ONLY_CAPABLE_CONNECTORS,
    }
}

fn pool(state: &AppState) -> Result<&sqlx::PgPool, SettingsApiError> {
    pool_from_state(state).map_err(|status| SettingsApiError {
        status,
        message: "database unavailable".into(),
    })
}

async fn get_knowledge_settings(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<KnowledgeSettingsResponse>, SettingsApiError> {
    let agent = WorkspaceSettingsService::new(pool(&state)?)
        .knowledge_compaction_agent()
        .await?;
    Ok(Json(settings_response(agent)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFileResponse {
    path: String,
    text: String,
    revision: String,
    backup_available: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveConfigBody {
    text: String,
    base_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveConfigResponse {
    revision: String,
    restart_required: bool,
    backup_available: bool,
}

#[derive(Serialize)]
struct BackupResponse {
    text: String,
}

struct ConfigApiError(ConfigFileError);

impl From<ConfigFileError> for ConfigApiError {
    fn from(error: ConfigFileError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ConfigApiError {
    fn into_response(self) -> Response {
        match self.0 {
            ConfigFileError::NoPath => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "message": "Config file is not available." })),
            )
                .into_response(),
            ConfigFileError::Invalid {
                line,
                column,
                message,
            } => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "line": line, "column": column, "message": message })),
            )
                .into_response(),
            ConfigFileError::Conflict { text, revision } => (
                StatusCode::CONFLICT,
                Json(json!({ "text": text, "revision": revision })),
            )
                .into_response(),
            ConfigFileError::BackupMissing => (
                StatusCode::NOT_FOUND,
                Json(json!({ "message": "No backup yet." })),
            )
                .into_response(),
            ConfigFileError::Io(error) => {
                tracing::error!(error = %error, "config file io error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "message": "Could not read the config file." })),
                )
                    .into_response()
            }
        }
    }
}

fn display_path(path: &Path) -> String {
    if let Ok(canonical) = path.canonicalize() {
        return canonical.display().to_string();
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return parent.join(name).display().to_string();
        }
    }
    path.display().to_string()
}

fn config_response(
    path: &Path,
    text: String,
    revision: String,
    backup_available: bool,
) -> ConfigFileResponse {
    ConfigFileResponse {
        path: display_path(path),
        text,
        revision,
        backup_available,
    }
}

async fn get_config_file(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
) -> Result<Json<ConfigFileResponse>, ConfigApiError> {
    let (path, document) = state.connectors.read_config_document()?;
    Ok(Json(config_response(
        &path,
        document.text,
        document.revision,
        document.backup_available,
    )))
}

async fn put_config_file(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<SaveConfigBody>,
) -> Result<Json<SaveConfigResponse>, ConfigApiError> {
    let saved = state
        .connectors
        .save_config_text(&body.text, &body.base_revision)?;
    Ok(Json(SaveConfigResponse {
        revision: saved.revision,
        restart_required: saved.restart_required,
        backup_available: saved.backup_available,
    }))
}

async fn get_config_backup(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
) -> Result<Json<BackupResponse>, ConfigApiError> {
    let text = state.connectors.read_backup_text()?;
    Ok(Json(BackupResponse { text }))
}

async fn put_knowledge_settings(
    State(state): State<Arc<AppState>>,
    AdminUser(auth): AdminUser,
    Json(body): Json<KnowledgeSettingsBody>,
) -> Result<Json<KnowledgeSettingsResponse>, SettingsApiError> {
    let agent = WorkspaceSettingsService::new(pool(&state)?)
        .set_knowledge_compaction_agent(body.compaction_agent_id, auth.user.id)
        .await?;
    Ok(Json(settings_response(agent)))
}
