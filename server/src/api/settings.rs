use crate::api::auth::{pool_from_state, AuthUser};
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
use std::sync::Arc;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/settings/knowledge",
        get(get_knowledge_settings).put(put_knowledge_settings),
    )
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
