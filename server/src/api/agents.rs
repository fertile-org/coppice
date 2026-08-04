use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::agent::{Agent, AgentPreset};
use crate::services::agent_health::{health_status_to_str, AgentHealthRegistry};
use crate::services::agent_service::{AgentError, AgentService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/agent-presets", get(list_presets))
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{agent_id}",
            get(get_agent)
                .patch(update_agent)
                .delete(delete_agent),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresetResponse {
    id: Uuid,
    key: String,
    role: String,
    skills: Vec<String>,
    responsibilities: Vec<String>,
    system_prompt_template: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponse {
    id: Uuid,
    name: String,
    role: String,
    skills: Vec<String>,
    responsibilities: Vec<String>,
    system_prompt: String,
    connector: String,
    model_provider: Option<String>,
    model: Option<String>,
    health: String,
    health_detail: Option<String>,
    enabled: bool,
    preset_source: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresetListResponse {
    items: Vec<PresetResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentListResponse {
    items: Vec<AgentResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAgentBody {
    name: String,
    preset_id: Option<Uuid>,
    role: Option<String>,
    skills: Option<Vec<String>>,
    responsibilities: Option<Vec<String>>,
    system_prompt: Option<String>,
    connector: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAgentBody {
    name: Option<String>,
    role: Option<String>,
    skills: Option<Vec<String>>,
    responsibilities: Option<Vec<String>>,
    system_prompt: Option<String>,
    connector: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
    enabled: Option<bool>,
}

fn preset_to_response(preset: AgentPreset, templates: &HashMap<String, String>) -> PresetResponse {
    let system_prompt_template = templates
        .get(&preset.key)
        .cloned()
        .unwrap_or_default();
    PresetResponse {
        id: preset.id,
        key: preset.key,
        role: preset.role,
        skills: preset.skills,
        responsibilities: preset.responsibilities,
        system_prompt_template,
    }
}

fn agent_to_response(agent: Agent, health: &AgentHealthRegistry) -> AgentResponse {
    let record = health.get(agent.id);
    AgentResponse {
        id: agent.id,
        name: agent.name,
        role: agent.role,
        skills: agent.skills,
        responsibilities: agent.responsibilities,
        system_prompt: agent.system_prompt,
        connector: agent.connector,
        model_provider: agent.model_provider,
        model: agent.model,
        health: health_status_to_str(record.status).into(),
        health_detail: record.detail,
        enabled: agent.enabled,
        preset_source: agent.preset_source,
        created_at: agent.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: agent.updated_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: AgentError) -> StatusCode {
    match err {
        AgentError::AgentNotFound | AgentError::PresetNotFound => StatusCode::NOT_FOUND,
        AgentError::Validation(_) => StatusCode::BAD_REQUEST,
        AgentError::KnowledgeProvenanceConflict => StatusCode::CONFLICT,
        AgentError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_presets(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<PresetListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);
    let presets = service.list_presets().await.map_err(map_error)?;
    Ok(Json(PresetListResponse {
        items: presets
            .into_iter()
            .map(|p| preset_to_response(p, &state.agent_templates))
            .collect(),
    }))
}

async fn list_agents(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<AgentListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);
    let agents = service.list_agents().await.map_err(map_error)?;
    Ok(Json(AgentListResponse {
        items: agents
            .into_iter()
            .map(|agent| agent_to_response(agent, &state.agent_health))
            .collect(),
    }))
}

async fn create_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Json(body): Json<CreateAgentBody>,
) -> Result<(StatusCode, Json<AgentResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);

    let agent = if let Some(preset_id) = body.preset_id {
        let preset = service.get_preset(preset_id).await.map_err(map_error)?;
        let default_prompt = state
            .agent_templates
            .get(&preset.key)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let system_prompt = body
            .system_prompt
            .as_deref()
            .unwrap_or(default_prompt.as_str());
        service
            .create_from_preset(
                preset_id,
                &body.name,
                system_prompt,
                body.connector.as_deref(),
                body.model_provider.as_deref(),
                body.model.as_deref(),
                body.enabled,
            )
            .await
            .map_err(map_error)?
    } else {
        let role = body
            .role
            .as_deref()
            .ok_or(StatusCode::BAD_REQUEST)?;
        let system_prompt = body
            .system_prompt
            .as_deref()
            .ok_or(StatusCode::BAD_REQUEST)?;
        service
            .create(
                &body.name,
                role,
                body.skills.as_deref().unwrap_or(&[]),
                body.responsibilities.as_deref().unwrap_or(&[]),
                system_prompt,
                body.connector.as_deref(),
                body.model_provider.as_deref(),
                body.model.as_deref(),
                body.enabled,
            )
            .await
            .map_err(map_error)?
    };

    Ok((StatusCode::CREATED, Json(agent_to_response(agent, &state.agent_health))))
}

async fn get_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<AgentResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);
    let agent = service.get(agent_id).await.map_err(map_error)?;
    Ok(Json(agent_to_response(agent, &state.agent_health)))
}

async fn update_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<UpdateAgentBody>,
) -> Result<Json<AgentResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);
    let agent = service
        .update(
            agent_id,
            body.name.as_deref(),
            body.role.as_deref(),
            body.skills.as_deref(),
            body.responsibilities.as_deref(),
            body.system_prompt.as_deref(),
            body.connector.as_deref(),
            body.model_provider.as_deref(),
            body.model.as_deref(),
            body.enabled,
        )
        .await
        .map_err(map_error)?;
    Ok(Json(agent_to_response(agent, &state.agent_health)))
}

async fn delete_agent(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = AgentService::new(pool);
    service.delete(agent_id).await.map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}
