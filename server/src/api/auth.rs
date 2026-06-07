use crate::domain::session::Session;
use crate::domain::user::User;
use crate::services::auth_service::{AuthError, AuthService};
use crate::AppState;
use axum::{
    extract::{FromRequestParts, State},
    http::{header::SET_COOKIE, request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user: User,
    pub session: Session,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/auth/bootstrap", post(bootstrap))
        .route("/api/auth/login", post(login))
}

pub fn protected_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
}

#[derive(Deserialize)]
struct BootstrapBody {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct UserResponse {
    id: uuid::Uuid,
    email: String,
    role: String,
}

#[derive(Serialize)]
struct LoginResponse {
    user: UserResponse,
    #[serde(rename = "csrfToken")]
    csrf_token: String,
}

pub fn pool_from_state(state: &AppState) -> Result<&sqlx::PgPool, StatusCode> {
    state.db.as_ref().ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

async fn bootstrap(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BootstrapBody>,
) -> Result<Response, StatusCode> {
    let bootstrap_password = headers
        .get("x-bootstrap-password")
        .and_then(|value| value.to_str().ok());

    if bootstrap_password != Some(state.config.auth.bootstrap_password.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }

    let pool = pool_from_state(&state)?;
    let auth = AuthService::new(pool, &state.config.auth);

    match auth.bootstrap_admin(&body.email, &body.password).await {
        Ok(user) => Ok(Json(UserResponse {
            id: user.id,
            email: user.email,
            role: user.role,
        })
        .into_response()),
        Err(AuthError::BootstrapNotAllowed) => Err(StatusCode::FORBIDDEN),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> Result<Response, StatusCode> {
    let pool = pool_from_state(&state)?;
    let auth = AuthService::new(pool, &state.config.auth);

    let bundle = auth
        .login(&body.email, &body.password)
        .await
        .map_err(|err| match err {
            AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    let cookie = session_cookie(&bundle.session_token, state.config.auth.cookie_secure);
    let body = LoginResponse {
        user: UserResponse {
            id: bundle.user.id,
            email: bundle.user.email,
            role: bundle.user.role,
        },
        csrf_token: bundle.session.csrf_token,
    };

    Ok((
        StatusCode::OK,
        [(SET_COOKIE, cookie)],
        Json(body),
    )
        .into_response())
}

async fn me(AuthUser { user, .. }: AuthUser) -> Json<UserResponse> {
    Json(UserResponse {
        id: user.id,
        email: user.email,
        role: user.role,
    })
}

async fn logout(
    State(state): State<Arc<AppState>>,
    AuthUser { session, .. }: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let pool = pool_from_state(&state)?;
    let auth = AuthService::new(pool, &state.config.auth);
    auth.logout(session.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

fn session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "coppice_session={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=604800"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}
