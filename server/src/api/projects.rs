use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::project::Project;
use crate::services::project_service::{ProjectError, ProjectService};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{project_id}",
            get(get_project).patch(update_project),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectResponse {
    id: Uuid,
    name: String,
    slug: String,
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectBody {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProjectBody {
    name: Option<String>,
}

fn project_to_response(project: Project) -> ProjectResponse {
    ProjectResponse {
        id: project.id,
        name: project.name,
        slug: project.slug,
        created_at: project
            .created_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    }
}

fn map_error(err: ProjectError) -> StatusCode {
    match err {
        ProjectError::ProjectNotFound => StatusCode::NOT_FOUND,
        ProjectError::RepoNotFound => StatusCode::NOT_FOUND,
        ProjectError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_projects(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<ProjectResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let projects = service
        .list_projects()
        .await
        .map_err(map_error)?;
    Ok(Json(
        projects.into_iter().map(project_to_response).collect(),
    ))
}

async fn create_project(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Json(body): Json<CreateProjectBody>,
) -> Result<(StatusCode, Json<ProjectResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let project = service
        .create_project(&body.name)
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(project_to_response(project))))
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<ProjectResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let project = service
        .get_project(project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(project_to_response(project)))
}

async fn update_project(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<UpdateProjectBody>,
) -> Result<Json<ProjectResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let project = service
        .update_project(project_id, body.name.as_deref())
        .await
        .map_err(map_error)?;
    Ok(Json(project_to_response(project)))
}
