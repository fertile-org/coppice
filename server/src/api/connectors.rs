use crate::api::auth::AuthUser;
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
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
struct ConnectorResponse {
    id: String,
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

async fn list_connectors(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Json<ConnectorListResponse> {
    let items = state
        .connector_registry
        .configured_ids()
        .into_iter()
        .map(|id| ConnectorResponse { id })
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
        .model_providers_for(&connector_id)
        .into_iter()
        .map(|id| ModelProviderResponse { id })
        .collect();
    Ok(Json(ModelProviderListResponse { items }))
}

async fn list_models(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path((connector_id, model_provider_id)): Path<(String, String)>,
) -> Result<Json<ModelListResponse>, StatusCode> {
    if !state.connector_registry.has(&connector_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !state
        .connector_registry
        .has_model_provider(&connector_id, &model_provider_id)
    {
        return Err(StatusCode::NOT_FOUND);
    }
    match connector_id.as_str() {
        "opencode" => {
            let command = &state.config.agent.connectors.opencode.command;
            let models = crate::providers::opencode_models::list_opencode_models(
                command,
                &model_provider_id,
            )
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
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
        "claude-code" => {
            let models = known_claude_code_models(&model_provider_id);
            Ok(Json(ModelListResponse {
                items: models
                    .into_iter()
                    .map(|m| ModelResponse {
                        id: m.id.to_string(),
                        name: m.name.to_string(),
                    })
                    .collect(),
            }))
        }
        "codex" => {
            let models = known_codex_models(&model_provider_id);
            Ok(Json(ModelListResponse {
                items: models
                    .into_iter()
                    .map(|m| ModelResponse {
                        id: m.id.to_string(),
                        name: m.name.to_string(),
                    })
                    .collect(),
            }))
        }
        "mock" => Ok(Json(ModelListResponse { items: vec![] })),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

struct KnownModel {
    id: &'static str,
    name: &'static str,
}

fn known_claude_code_models(provider_id: &str) -> Vec<KnownModel> {
    match provider_id {
        "sonnet" => vec![
            KnownModel { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4" },
            KnownModel { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet" },
        ],
        "opus" => vec![
            KnownModel { id: "claude-opus-4-20250514", name: "Claude Opus 4" },
            KnownModel { id: "claude-3-opus-20240229", name: "Claude 3 Opus" },
        ],
        "haiku" => vec![
            KnownModel { id: "claude-3-5-haiku-20241022", name: "Claude 3.5 Haiku" },
            KnownModel { id: "claude-3-haiku-20240307", name: "Claude 3 Haiku" },
        ],
        _ => vec![],
    }
}

fn known_codex_models(provider_id: &str) -> Vec<KnownModel> {
    match provider_id {
        "openai" => vec![
            KnownModel { id: "gpt-4o", name: "GPT-4o" },
            KnownModel { id: "gpt-4o-mini", name: "GPT-4o Mini" },
            KnownModel { id: "o1", name: "o1" },
            KnownModel { id: "o1-mini", name: "o1 Mini" },
        ],
        "azure" => vec![
            KnownModel { id: "azure/gpt-4o", name: "Azure GPT-4o" },
            KnownModel { id: "azure/gpt-4o-mini", name: "Azure GPT-4o Mini" },
            KnownModel { id: "azure/o1", name: "Azure o1" },
        ],
        _ => vec![],
    }
}
