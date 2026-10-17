use crate::api::auth::pool_from_state;
use crate::middleware::admin::AdminUser;
use crate::services::init_wizard::{
    detect_connector, Detection, InitError, InitInput, InitWizardService, STARTER_PRESET_ID,
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
    Router::new().route("/api/init", get(preview).post(confirm))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResponse {
    preset_id: &'static str,
    status: &'static str,
    connectors: Vec<ConnectorResponse>,
    members: Vec<MemberResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorResponse {
    id: String,
    display_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemberResponse {
    preset_key: String,
    name: String,
    role: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmBody {
    preset_id: String,
    board_name: String,
    connector_id: String,
    repository: RepositoryBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryBody {
    name: String,
    local_path: String,
    remote_url: Option<String>,
    #[serde(default = "default_branch")]
    default_branch: String,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmResponse {
    board_id: Uuid,
    repository_id: Uuid,
    connector_id: String,
    connector_display_name: String,
    connector_turned_on: bool,
    agents: Vec<CreatedAgentResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedAgentResponse {
    id: Uuid,
    name: String,
    preset_key: String,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
}

fn desktop_or_hide(state: &AppState) -> Result<(), StatusCode> {
    if state.config.auth.desktop_mode {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

fn error_response(status: StatusCode, code: &'static str) -> Response {
    (status, Json(ErrorBody { code })).into_response()
}

fn map_init_error(err: InitError) -> Response {
    match err {
        InitError::Checking => error_response(StatusCode::CONFLICT, "checking"),
        InitError::NoneReady => error_response(StatusCode::CONFLICT, "none_ready"),
        InitError::UnknownPreset => error_response(StatusCode::BAD_REQUEST, "unknown_preset"),
        InitError::NotGit => error_response(StatusCode::BAD_REQUEST, "not_git"),
        InitError::DuplicatePath => error_response(StatusCode::CONFLICT, "duplicate_path"),
        InitError::DuplicateBoard => error_response(StatusCode::CONFLICT, "duplicate_board"),
        InitError::Validation(_) => error_response(StatusCode::BAD_REQUEST, "validation"),
        InitError::PresetUnavailable | InitError::Database(_) => {
            tracing::warn!(error = %err, "init wizard failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn preview(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<Response, StatusCode> {
    desktop_or_hide(&state)?;
    let pool = pool_from_state(&state)?;
    let service = InitWizardService::new(pool, &state.agent_templates);
    let members = service.team_members().await.map_err(|err| {
        tracing::warn!(error = %err, "init wizard preview failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let detection = detect_connector(|id| state.connector_probes.readiness(id));
    let (status, connectors) = match detection {
        Detection::Checking => ("checking", Vec::new()),
        Detection::NoneReady => ("none", Vec::new()),
        Detection::Ready(connectors) => (
            "ready",
            connectors
                .into_iter()
                .map(|connector| ConnectorResponse {
                    id: connector.id.to_string(),
                    display_name: connector.display_name.to_string(),
                })
                .collect(),
        ),
    };
    Ok(Json(PreviewResponse {
        preset_id: STARTER_PRESET_ID,
        status,
        connectors,
        members: members
            .into_iter()
            .map(|member| MemberResponse {
                preset_key: member.preset_key,
                name: member.name,
                role: member.role,
            })
            .collect(),
    })
    .into_response())
}

async fn confirm(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<ConfirmBody>,
) -> Result<Response, StatusCode> {
    desktop_or_hide(&state)?;
    let pool = pool_from_state(&state)?;
    let detection = detect_connector(|id| state.connector_probes.readiness(id));
    let connector = match detection {
        Detection::Checking => return Ok(map_init_error(InitError::Checking)),
        Detection::NoneReady => return Ok(map_init_error(InitError::NoneReady)),
        Detection::Ready(connectors) => {
            let chosen = body.connector_id;
            match connectors
                .into_iter()
                .find(|connector| connector.id == chosen)
            {
                Some(connector) => connector,
                None => return Ok(map_init_error(InitError::Validation("connector_id"))),
            }
        }
    };
    let service = InitWizardService::new(pool, &state.agent_templates);
    let created = match service
        .create(
            InitInput {
                preset_id: body.preset_id,
                board_name: body.board_name,
                repo_name: body.repository.name,
                local_path: body.repository.local_path,
                remote_url: body.repository.remote_url,
                default_branch: body.repository.default_branch,
            },
            connector.id,
        )
        .await
    {
        Ok(created) => created,
        Err(err) => return Ok(map_init_error(err)),
    };

    let connector_turned_on = match state.connectors.set_enabled(connector.id, true) {
        Ok(change) => change.changed,
        Err(err) => {
            tracing::warn!(error = %err, connector = connector.id, "init wizard did not turn connector on");
            false
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(ConfirmResponse {
            board_id: created.board_id,
            repository_id: created.repository_id,
            connector_id: connector.id.to_string(),
            connector_display_name: connector.display_name.to_string(),
            connector_turned_on,
            agents: created
                .agents
                .into_iter()
                .map(|agent| CreatedAgentResponse {
                    id: agent.id,
                    name: agent.name,
                    preset_key: agent.preset_key,
                })
                .collect(),
        }),
    )
        .into_response())
}
