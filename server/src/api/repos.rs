use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::repo::Repo;
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
        .route(
            "/api/projects/{project_id}/repos",
            get(list_repos).post(create_repo),
        )
        .route(
            "/api/repos/{repo_id}",
            get(get_repo)
                .patch(update_repo)
                .delete(delete_repo),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoResponse {
    id: Uuid,
    project_id: Uuid,
    name: String,
    remote_url: Option<String>,
    default_branch: String,
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRepoBody {
    name: String,
    remote_url: Option<String>,
    #[serde(default = "default_branch")]
    default_branch: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRepoBody {
    name: Option<String>,
    remote_url: Option<String>,
    default_branch: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn repo_to_response(repo: Repo) -> RepoResponse {
    RepoResponse {
        id: repo.id,
        project_id: repo.project_id,
        name: repo.name,
        remote_url: repo.remote_url,
        default_branch: repo.default_branch,
        created_at: repo.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: ProjectError) -> StatusCode {
    match err {
        ProjectError::ProjectNotFound | ProjectError::RepoNotFound => StatusCode::NOT_FOUND,
        ProjectError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_repos(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<RepoResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let repos = service
        .list_repos(project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(repos.into_iter().map(repo_to_response).collect()))
}

async fn create_repo(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<CreateRepoBody>,
) -> Result<(StatusCode, Json<RepoResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let repo = service
        .create_repo(
            project_id,
            &body.name,
            body.remote_url.as_deref(),
            &body.default_branch,
        )
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(repo_to_response(repo))))
}

async fn get_repo(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<RepoResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let repo = service.get_repo(repo_id).await.map_err(map_error)?;
    Ok(Json(repo_to_response(repo)))
}

async fn update_repo(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<UpdateRepoBody>,
) -> Result<Json<RepoResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    let remote_url = body.remote_url.as_ref().map(|url| Some(url.as_str()));
    let repo = service
        .update_repo(
            repo_id,
            body.name.as_deref(),
            remote_url,
            body.default_branch.as_deref(),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(repo_to_response(repo)))
}

async fn delete_repo(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(repo_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = ProjectService::new(pool);
    service.delete_repo(repo_id).await.map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}
