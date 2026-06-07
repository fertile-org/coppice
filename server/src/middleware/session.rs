use crate::api::auth::AuthUser;
use crate::services::auth_service::{AuthError, AuthService};
use crate::AppState;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

pub async fn session_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(pool) = state.db.as_ref() else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    let session_token = request
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_session_cookie);

    let Some(session_token) = session_token else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let auth = AuthService::new(pool, &state.config.auth);
    match auth.user_by_session_token(&session_token).await {
        Ok((user, session)) => {
            request.extensions_mut().insert(AuthUser { user, session });
            Ok(next.run(request).await)
        }
        Err(AuthError::SessionNotFound) | Err(AuthError::InvalidCredentials) => {
            Err(StatusCode::UNAUTHORIZED)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub fn parse_session_cookie(cookie_header: &str) -> Option<String> {
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("coppice_session="))
        .map(str::to_string)
}
