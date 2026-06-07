use crate::api::auth::pool_from_state;
use crate::domain::user::User;
use crate::middleware::admin::AdminUser;
use crate::services::user_service::{UserError, UserService};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/users", get(list_users).post(create_user))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserResponse {
    id: uuid::Uuid,
    email: String,
    role: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserListResponse {
    items: Vec<UserResponse>,
}

#[derive(Deserialize)]
struct CreateUserBody {
    email: String,
    password: String,
}

fn user_to_response(user: User) -> UserResponse {
    UserResponse {
        id: user.id,
        email: user.email,
        role: user.role,
        created_at: user.created_at.format(&Rfc3339).unwrap_or_default(),
    }
}

fn map_error(err: UserError) -> StatusCode {
    match err {
        UserError::EmailTaken => StatusCode::CONFLICT,
        UserError::PasswordHash => StatusCode::INTERNAL_SERVER_ERROR,
        UserError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
) -> Result<Json<UserListResponse>, StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = UserService::new(pool);
    let users = service.list_users().await.map_err(map_error)?;
    Ok(Json(UserListResponse {
        items: users.into_iter().map(user_to_response).collect(),
    }))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    AdminUser(_): AdminUser,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<UserResponse>), StatusCode> {
    let pool = pool_from_state(&state)?;
    let service = UserService::new(pool);
    let user = service
        .create_member(&body.email, &body.password)
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(user_to_response(user))))
}
