use crate::api::auth::{pool_from_state, AuthUser};
use crate::domain::repo::{verification_status_to_str, Repo};
use crate::middleware::admin::AdminUser;
use crate::services::code_review_service::{
    BranchesResponse, CodeReviewError, CodeReviewService, DiffSummary, FilePatch,
};
use crate::services::repo_service::{RepoError, RepoService};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/repos", get(list_repos).post(create_repo))
        .route(
            "/api/repos/{repo_id}",
            get(get_repo).patch(update_repo).delete(delete_repo),
        )
        .route("/api/repos/{repo_id}/verify", post(verify_repo))
        .route("/api/repos/{repo_id}/worktrees", get(list_repo_worktrees))
        .route("/api/repos/{repo_id}/branches", get(list_repo_branches))
        .route("/api/repos/{repo_id}/diff", get(get_repo_diff))
        .route("/api/repos/{repo_id}/diff/file", get(get_repo_diff_file))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffQuery {
    worktree_path: String,
    base_branch: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffFileQuery {
    worktree_path: String,
    base_branch: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoResponse {
    id: Uuid,
    name: String,
    local_path: String,
    remote_url: Option<String>,
    default_branch: String,
    verification_status: String,
    verification_error: Option<String>,
    last_verified_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRepoBody {
    name: String,
    local_path: String,
    remote_url: Option<String>,
    #[serde(default = "default_branch")]
    default_branch: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRepoBody {
    name: Option<String>,
    local_path: Option<String>,
    remote_url: Option<String>,
    default_branch: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn repo_to_response(repo: Repo) -> RepoResponse {
    RepoResponse {
        id: repo.id,
        name: repo.name,
        local_path: repo.local_path,
        remote_url: repo.remote_url,
        default_branch: repo.default_branch,
        verification_status: verification_status_to_str(repo.verification_status).to_string(),
        verification_error: repo.verification_error,
        last_verified_at: repo
            .last_verified_at
            .map(|t| t.format(&Rfc3339).unwrap_or_default()),
        created_at: repo.created_at.format(&Rfc3339).unwrap_or_default(),
        updated_at: repo.updated_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: RepoError) -> StatusCode {
    match err {
        RepoError::NotFound => StatusCode::NOT_FOUND,
        RepoError::InUse | RepoError::DuplicatePath => StatusCode::CONFLICT,
        RepoError::Validation(_) => StatusCode::BAD_REQUEST,
        RepoError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_repos(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
) -> Result<Json<Vec<RepoResponse>>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RepoService::new(pool);
    let repos = service.list_all().await.map_err(map_error)?;
    Ok(Json(repos.into_iter().map(repo_to_response).collect()))
}

async fn create_repo(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<CreateRepoBody>,
) -> Result<(StatusCode, Json<RepoResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RepoService::new(pool);
    let repo = service
        .create(
            &body.name,
            &body.local_path,
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
    let service = RepoService::new(pool);
    let repo = service.get(repo_id).await.map_err(map_error)?;
    Ok(Json(repo_to_response(repo)))
}

async fn update_repo(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(repo_id): Path<Uuid>,
    Json(body): Json<UpdateRepoBody>,
) -> Result<Json<RepoResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RepoService::new(pool);
    let remote_url = body.remote_url.as_ref().map(|url| Some(url.as_str()));
    let repo = service
        .update(
            repo_id,
            body.name.as_deref(),
            body.local_path.as_deref(),
            remote_url,
            body.default_branch.as_deref(),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(repo_to_response(repo)))
}

async fn delete_repo(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(repo_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RepoService::new(pool);
    service.delete(repo_id).await.map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn verify_repo(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<RepoResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = RepoService::new(pool);
    let repo = service.verify(repo_id).await.map_err(map_error)?;
    Ok(Json(repo_to_response(repo)))
}

async fn list_repo_worktrees(
    AuthUser { .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let worktrees = service
        .list_worktrees(repo_id)
        .await
        .map_err(map_code_review_error)?;
    Ok(Json(serde_json::json!({ "worktrees": worktrees })))
}

async fn list_repo_branches(
    AuthUser { .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_id): Path<Uuid>,
) -> Result<Json<BranchesResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let branches = service
        .list_branches(repo_id)
        .await
        .map_err(map_code_review_error)?;
    Ok(Json(branches))
}

async fn get_repo_diff(
    AuthUser { .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_id): Path<Uuid>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<DiffSummary>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let diff = service
        .diff_summary(repo_id, &query.worktree_path, &query.base_branch)
        .await
        .map_err(map_code_review_error)?;
    Ok(Json(diff))
}

async fn get_repo_diff_file(
    AuthUser { .. }: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_id): Path<Uuid>,
    Query(query): Query<DiffFileQuery>,
) -> Result<Json<FilePatch>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = CodeReviewService::new(
        pool,
        state.config.agent.worktrees_path.clone().into(),
    );
    let patch = service
        .file_patch(
            repo_id,
            &query.worktree_path,
            &query.base_branch,
            &query.path,
        )
        .await
        .map_err(map_code_review_error)?;
    Ok(Json(patch))
}

fn map_code_review_error(err: CodeReviewError) -> StatusCode {
    match err {
        CodeReviewError::RepoNotFound | CodeReviewError::TicketNotFound => StatusCode::NOT_FOUND,
        CodeReviewError::RepoNotReady
        | CodeReviewError::InvalidWorktreePath
        | CodeReviewError::InvalidFilePath
        | CodeReviewError::InvalidBranchName => StatusCode::BAD_REQUEST,
        CodeReviewError::PatchTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
