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
struct ModelsQuery {
    /// When true, skip the server's cached list for this connector and provider.
    #[serde(default)]
    refresh: bool,
}

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Json<ConnectorListResponse> {
    let registry = state.connectors.registry();
    let items = coppice_connectors::all()
        .iter()
        .filter(|descriptor| registry.has(descriptor.id))
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
                enabled: true,
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

async fn list_model_providers(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(connector_id): Path<String>,
) -> Result<Json<ModelProviderListResponse>, ModelsApiError> {
    let registry = state.connectors.registry();
    if !registry.has(&connector_id) {
        if let Some(err) = turned_off_error(&state, &connector_id) {
            return Err(err);
        }
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
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
        if let Some(err) = turned_off_error(&state, &connector_id) {
            return Err(err);
        }
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    if !registry
        .has_model_provider(&connector_id, &model_provider_id)
        .await
    {
        let message = format!("Unknown model provider '{model_provider_id}'.");
        tracing::warn!("{message}");
        return Err(ModelsApiError::Logged {
            status: StatusCode::NOT_FOUND,
            message,
        });
    }
    let catalog = registry
        .models(&connector_id)
        .ok_or(ModelsApiError::Status(StatusCode::NOT_FOUND))?;
    let models = catalog
        .list_models(&model_provider_id, query.refresh)
        .await
        .map_err(|err| models_gateway_err(&format!("{connector_id} models"), &err))?;
    if models.is_empty() {
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
    tracing::info!(
        connector = %connector_id,
        model_provider = %model_provider_id,
        count = models.len(),
        refresh = query.refresh,
        "listed models"
    );
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
