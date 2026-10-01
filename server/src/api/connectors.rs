use crate::api::auth::AuthUser;
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use coppice_connectors::ConsoleKind;
use serde::Serialize;
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
}

enum ModelsApiError {
    Status(StatusCode),
    Message(StatusCode, String),
}

impl IntoResponse for ModelsApiError {
    fn into_response(self) -> Response {
        match self {
            ModelsApiError::Status(code) => code.into_response(),
            ModelsApiError::Message(code, message) => {
                tracing::warn!(%message, status = %code, "connector models listing failed");
                (code, Json(ApiMessageResponse { message })).into_response()
            }
        }
    }
}

fn models_gateway_err(context: &str, err: impl std::fmt::Display) -> ModelsApiError {
    ModelsApiError::Message(StatusCode::BAD_GATEWAY, format!("{context}: {err}"))
}

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Json<ConnectorListResponse> {
    let items = state
        .connector_registry
        .configured_ids()
        .into_iter()
        .filter_map(|id| {
            let descriptor = crate::providers::descriptor(&id)?;
            Some(ConnectorResponse {
                id,
                display_name: descriptor.display_name,
                console: descriptor.console,
                caps: ConnectorCapsResponse {
                    read_only_tools: descriptor.caps.read_only_tools,
                    chat_resume: descriptor.caps.chat_resume,
                },
            })
        })
        .collect();
    Json(ConnectorListResponse { items })
}

async fn list_model_providers(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(connector_id): Path<String>,
) -> Result<Json<ModelProviderListResponse>, StatusCode> {
    if !state.connector_registry.has(&connector_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let items = state
        .connector_registry
        .models(&connector_id)
        .map(|models| models.model_providers().to_vec())
        .unwrap_or_default()
        .into_iter()
        .map(|id| ModelProviderResponse { id })
        .collect();
    Ok(Json(ModelProviderListResponse { items }))
}

async fn list_models(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path((connector_id, model_provider_id)): Path<(String, String)>,
) -> Result<Json<ModelListResponse>, ModelsApiError> {
    if !state.connector_registry.has(&connector_id) {
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    if !state
        .connector_registry
        .has_model_provider(&connector_id, &model_provider_id)
    {
        return Err(ModelsApiError::Status(StatusCode::NOT_FOUND));
    }
    let catalog = state
        .connector_registry
        .models(&connector_id)
        .ok_or(ModelsApiError::Status(StatusCode::NOT_FOUND))?;
    let models = catalog
        .list_models(&model_provider_id)
        .await
        .map_err(|err| models_gateway_err(&format!("{connector_id} models"), err))?;
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
