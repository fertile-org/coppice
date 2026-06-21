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
            let models = crate::providers::codex_models::list_codex_models(&model_provider_id)
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
        "kilo-code" => {
            let command = &state.config.agent.connectors.kilo_code.command;
            let models = crate::providers::kilo_models::list_kilo_models(
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
        "mock" => Ok(Json(ModelListResponse { items: vec![] })),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

struct KnownModel {
    id: &'static str,
    name: &'static str,
}

/// Curated Claude Code model IDs (Anthropic API / subscription CLI).
/// No live CLI catalog exists yet; refresh from https://platform.claude.com/docs/en/about-claude/models/overview
/// Fable 5 is intentionally omitted (not permitted in this deployment).
fn known_claude_code_models(provider_id: &str) -> Vec<KnownModel> {
    match provider_id {
        "sonnet" => vec![
            KnownModel {
                id: "claude-sonnet-4-6",
                name: "Claude Sonnet 4.6",
            },
            KnownModel {
                id: "sonnet[1m]",
                name: "Sonnet 4.6 (1M context)",
            },
            KnownModel {
                id: "claude-sonnet-4-5-20250929",
                name: "Claude Sonnet 4.5",
            },
        ],
        "opus" => vec![
            KnownModel {
                id: "claude-opus-4-8",
                name: "Claude Opus 4.8",
            },
            KnownModel {
                id: "opus[1m]",
                name: "Opus 4.8 (1M context)",
            },
            KnownModel {
                id: "claude-opus-4-7",
                name: "Claude Opus 4.7",
            },
            KnownModel {
                id: "claude-opus-4-6",
                name: "Claude Opus 4.6",
            },
        ],
        "haiku" => vec![
            KnownModel {
                id: "claude-haiku-4-5-20251001",
                name: "Claude Haiku 4.5",
            },
            KnownModel {
                id: "claude-haiku-4-5",
                name: "Claude Haiku 4.5 (alias)",
            },
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod claude_code_models_tests {
    use super::*;

    #[test]
    fn claude_code_models_include_current_opus_and_exclude_fable() {
        let opus = known_claude_code_models("opus");
        assert!(opus.iter().any(|m| m.id == "claude-opus-4-8"));
        assert!(!opus.iter().any(|m| m.id.contains("fable")));
        let sonnet = known_claude_code_models("sonnet");
        assert!(sonnet.iter().any(|m| m.id == "claude-sonnet-4-6"));
    }
}
