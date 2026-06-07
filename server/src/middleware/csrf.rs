use crate::api::auth::AuthUser;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};

const CSRF_HEADER: &str = "x-csrf-token";

pub async fn csrf_middleware(request: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    if !requires_csrf(&request) {
        return Ok(next.run(request).await);
    }

    let csrf_header = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok());

    let auth_user = request.extensions().get::<AuthUser>();

    match (csrf_header, auth_user) {
        (Some(header), Some(user)) if header == user.session.csrf_token => {
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::FORBIDDEN),
    }
}

fn requires_csrf(request: &Request<Body>) -> bool {
    matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) && request.uri().path().starts_with("/api/")
}
