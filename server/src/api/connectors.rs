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
    /// `message` was already logged at warn, exactly.
    Logged {
        status: StatusCode,
        message: String,
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
            ModelsApiError::Logged { status, message } => (
                status,
                Json(ApiMessageResponse {
                    message,
                    detail: None,
                }),
            )
                .into_response(),
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
struct RefreshQuery {
    /// When true, skip the cached CLI listing.
    /// Provider discovery honors this only for Refresh models.
    #[serde(default)]
    refresh: bool,
}

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Json<ConnectorListResponse> {
    let registry = state.connectors.registry();
    // OpenCode 2 runs use the structured console; the version decides.
    let opencode_console =
        crate::providers::run_console_kind(coppice_connectors::OPENCODE, &state.config).await;
    // Every connector, so the picker can show why one is unavailable. Mock is
    // listed only in builds that register it.
    let items = coppice_connectors::all()
        .iter()
        .filter(|descriptor| descriptor.id != MOCK || registry.has(MOCK))
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
                console: if descriptor.id == coppice_connectors::OPENCODE {
                    opencode_console.unwrap_or(descriptor.console)
                } else {
                    descriptor.console
                },
                caps: ConnectorCapsResponse {
                    read_only_tools: descriptor.caps.read_only_tools,
                    chat_resume: descriptor.caps.chat_resume,
                },
                enabled: registry.has(descriptor.id),
                readiness,
                docs_url: descriptor.install.docs_url,
            }
        })
        .collect();
    Json(ConnectorListResponse { items })
}

fn turned_off_error(state: &AppState, connector_id: &str) -> Option<ModelsApiError> {
    let descriptor = coppice_connectors::get(connector_id)?;
    if descriptor.id == MOCK || !state.connectors.explicitly_off(connector_id) {
        return None;
    }
    let message = crate::connectors_runtime::models_turned_off_log(descriptor.display_name);
    tracing::warn!("{message}");
    Some(ModelsApiError::Logged {
        status: StatusCode::CONFLICT,
        message,
    })
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
    Query(query): Query<RefreshQuery>,
) -> Result<Json<ModelProviderListResponse>, ModelsApiError> {
    let registry = state.connectors.registry();
    if !registry.has(&connector_id) {
        if let Some(err) = turned_off_error(&state, &connector_id) {
            return Err(err);
        }
        tracing::warn!(
            connector = %connector_id,
            status = 404_u16,
            "model providers not found"
        );
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    let items = match registry.models(&connector_id) {
        Some(models) => models.model_providers(query.refresh).await,
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
    Query(query): Query<RefreshQuery>,
) -> Result<Json<ModelListResponse>, ModelsApiError> {
    if query.refresh {
        crate::host_env::invalidate();
    }
    let registry = state.connectors.registry();
    if !registry.has(&connector_id) {
        if let Some(err) = turned_off_error(&state, &connector_id) {
            return Err(err);
        }
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
        Ok(models) if models.is_empty() => {
            let name = coppice_connectors::get(&connector_id)
                .map(|descriptor| descriptor.display_name)
                .unwrap_or(connector_id.as_str());
            let message = format!("{name} returned no models.");
            tracing::warn!("{message}");
            return Err(ModelsApiError::Logged {
                status: StatusCode::BAD_GATEWAY,
                message,
            });
        }
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
