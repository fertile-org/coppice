use crate::api::auth::AuthUser;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use coppice_connectors::sign_in::Readiness;
use coppice_connectors::{ConsoleKind, MOCK};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/connectors", get(list_connectors))
        .route(
            "/api/connectors/{connector_id}/model-providers",
            get(list_model_providers),
        )
        .route(
            "/api/connectors/{connector_id}/model-providers/{model_provider_id}/models",
            get(list_models),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorResponse {
    id: String,
    display_name: &'static str,
    console: ConsoleKind,
    caps: ConnectorCapsResponse,
    enabled: bool,
    /// `null` until the startup probe finishes. Mock is always `ready`.
    readiness: Option<Readiness>,
    docs_url: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorCapsResponse {
    read_only_tools: bool,
    chat_resume: bool,
}

#[derive(Serialize)]
struct ConnectorListResponse {
    items: Vec<ConnectorResponse>,
}

#[derive(Serialize)]
struct ModelProviderResponse {
    id: String,
}

#[derive(Serialize)]
struct ModelProviderListResponse {
    items: Vec<ModelProviderResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelResponse {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct ModelListResponse {
    items: Vec<ModelResponse>,
}

#[derive(Serialize)]
struct ApiMessageResponse {
    message: String,
    /// First CLI stderr line, when the list command failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

enum ModelsApiError {
    Status(StatusCode),
    Message {
        status: StatusCode,
        message: String,
        detail: Option<String>,
    },
}

impl IntoResponse for ModelsApiError {
    fn into_response(self) -> Response {
        match self {
            ModelsApiError::Status(code) => code.into_response(),
            ModelsApiError::Message {
                status,
                message,
                detail,
            } => {
                tracing::warn!(%message, ?detail, %status, "connector models listing failed");
                (status, Json(ApiMessageResponse { message, detail })).into_response()
            }
        }
    }
}

fn models_gateway_err(context: &str, err: &anyhow::Error) -> ModelsApiError {
    ModelsApiError::Message {
        status: StatusCode::BAD_GATEWAY,
        message: format!("{context}: {err}"),
        detail: crate::providers::cli_catalog::model_list_detail(err),
    }
}

#[derive(Debug, Deserialize, Default)]
struct ModelsQuery {
    /// When true, skip the server's cached list for this connector and provider.
    #[serde(default)]
    refresh: bool,
}

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Json<ConnectorListResponse> {
    let config = state.connectors.config();
    let items = coppice_connectors::all()
        .iter()
        .map(|descriptor| {
            let readiness = if descriptor.id == MOCK {
                Some(Readiness::Ready)
            } else {
                state
                    .connector_probes
                    .get(descriptor.id)
                    .map(|cached| cached.readiness)
            };
            ConnectorResponse {
                id: descriptor.id.to_string(),
                display_name: descriptor.display_name,
                console: descriptor.console,
                caps: ConnectorCapsResponse {
                    read_only_tools: descriptor.caps.read_only_tools,
                    chat_resume: descriptor.caps.chat_resume,
                },
                enabled: config
                    .agent
                    .connectors
                    .enabled(descriptor.id)
                    .unwrap_or(false),
                readiness,
                docs_url: descriptor.install.docs_url,
            }
        })
        .collect();
    Json(ConnectorListResponse { items })
}

fn model_list_program(state: &AppState, connector_id: &str) -> String {
    let config = state.connectors.config();
    let command = config
        .agent
        .connectors
        .command(connector_id)
        .map(str::to_string)
        .or_else(|| {
            coppice_connectors::get(connector_id).map(|descriptor| descriptor.binary.to_string())
        });
    let Some(command) = command else {
        return String::new();
    };
    match crate::host_env::current().resolve(&command) {
        Ok(path) => path.display().to_string(),
        Err(missing) => missing.to_string(),
    }
}

async fn list_model_providers(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(connector_id): Path<String>,
) -> Result<Json<ModelProviderListResponse>, StatusCode> {
    let registry = state.connectors.registry();
    if !registry.has(&connector_id) {
        tracing::warn!(
            connector = %connector_id,
            status = 404_u16,
            "model providers not found"
        );
        return Err(StatusCode::NOT_FOUND);
    }
    let items = match registry.models(&connector_id) {
        Some(models) => models.model_providers().await,
        None => Vec::new(),
    }
    .into_iter()
    .map(|id| ModelProviderResponse { id })
    .collect();
    Ok(Json(ModelProviderListResponse { items }))
}

async fn list_models(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path((connector_id, model_provider_id)): Path<(String, String)>,
    Query(query): Query<ModelsQuery>,
) -> Result<Json<ModelListResponse>, ModelsApiError> {
    if query.refresh {
        crate::host_env::invalidate();
    }
    let registry = state.connectors.registry();
    if !registry.has(&connector_id) {
        tracing::warn!(
            connector = %connector_id,
            provider = %model_provider_id,
            status = 404_u16,
            "models not found"
        );
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    if !registry
        .has_model_provider(&connector_id, &model_provider_id)
        .await
    {
        tracing::warn!(
            connector = %connector_id,
            provider = %model_provider_id,
            status = 404_u16,
            "models not found"
        );
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    let Some(catalog) = registry.models(&connector_id) else {
        tracing::warn!(
            connector = %connector_id,
            provider = %model_provider_id,
            status = 404_u16,
            "models not found"
        );
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    };
    let program = model_list_program(&state, &connector_id);
    let models = match catalog.list_models(&model_provider_id, query.refresh).await {
        Ok(models) => {
            tracing::info!(
                connector = %connector_id,
                provider = %model_provider_id,
                %program,
                count = models.len(),
                "listed models"
            );
            models
        }
        Err(err) => {
            tracing::info!(
                connector = %connector_id,
                provider = %model_provider_id,
                %program,
                error = %err,
                "listed models"
            );
            return Err(models_gateway_err(&format!("{connector_id} models"), &err));
        }
    };
    Ok(Json(ModelListResponse {
        items: models
            .into_iter()
            .map(|m| ModelResponse {
                id: m.id,
                name: m.name,
            })
            .collect(),
    }))
}
