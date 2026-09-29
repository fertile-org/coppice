use crate::mcp::host::RunToolHost;
use crate::mcp::protocol::handle_rpc;
use crate::mcp::token::TokenService;
use crate::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/mcp", post(post_mcp))
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty()).then(|| token.trim())
}

async fn post_mcp(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return unauthorized();
    };
    let Some(pool) = state.db.as_ref() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let scope = match TokenService::new(pool).verify(token).await {
        Ok(Some(scope)) => scope,
        Ok(None) => return unauthorized(),
        Err(e) => {
            tracing::error!(error = %e, "mcp token verification failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": "parse error" },
            })),
        )
            .into_response();
    };

    let host = RunToolHost::new(state, scope);
    match handle_rpc(request, &host).await {
        Some(response) => Json(response).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}
