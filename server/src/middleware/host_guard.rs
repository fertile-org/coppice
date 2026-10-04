use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header::HOST, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::config::AuthConfig;
use crate::AppState;

/// Rejects requests addressed to any host but the desktop app's own loopback
/// origin, so a DNS-rebound page cannot reach the API. Off unless
/// `auth.desktop_mode` is set and `auth.desktop_allowed_hosts` is non-empty.
pub async fn desktop_host_guard(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().authority().map(|a| a.as_str()));
    if !host_allowed(&state.config.auth, host) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}

fn guard_active(auth: &AuthConfig) -> bool {
    auth.desktop_mode && !auth.desktop_allowed_hosts.is_empty()
}

pub fn host_allowed(auth: &AuthConfig, host: Option<&str>) -> bool {
    if !guard_active(auth) {
        return true;
    }
    host.is_some_and(|host| {
        auth.desktop_allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    })
}

/// A present `Origin` must be `http://` plus an allowed host; an absent one
/// (non-browser client) passes.
pub fn origin_allowed(auth: &AuthConfig, headers: &HeaderMap) -> bool {
    if !guard_active(auth) {
        return true;
    }
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        return true;
    };
    let Some(host) = origin
        .to_str()
        .ok()
        .and_then(|origin| origin.strip_prefix("http://"))
    else {
        return false;
    };
    host_allowed(auth, Some(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::ORIGIN;
    use tower::ServiceExt;

    const PORT: u16 = 43210;

    fn desktop_auth() -> AuthConfig {
        let mut auth = crate::config::AppConfig::load_defaults()
            .expect("defaults")
            .auth;
        auth.desktop_mode = true;
        auth.desktop_allowed_hosts = vec![format!("127.0.0.1:{PORT}"), format!("localhost:{PORT}")];
        auth
    }

    async fn desktop_app() -> axum::Router {
        let state = crate::test_state().await;
        let mut state = (*state).clone();
        state.config.auth = desktop_auth();
        crate::app(Arc::new(state))
    }

    async fn status(app: &axum::Router, request: Request<Body>) -> StatusCode {
        app.clone().oneshot(request).await.unwrap().status()
    }

    fn get(host: &str, path: &str) -> Request<Body> {
        Request::builder()
            .uri(path)
            .header(HOST, host)
            .body(Body::empty())
            .unwrap()
    }

    fn desktop_session(host: &str, origin: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/auth/desktop-session")
            .header(HOST, host);
        if let Some(origin) = origin {
            builder = builder.header(ORIGIN, origin);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn host_check_is_off_without_desktop_mode_or_allowed_hosts() {
        let mut auth = desktop_auth();
        assert!(!host_allowed(&auth, Some("evil.example:43210")));
        auth.desktop_mode = false;
        assert!(host_allowed(&auth, Some("evil.example:43210")));
        auth.desktop_mode = true;
        auth.desktop_allowed_hosts.clear();
        assert!(host_allowed(&auth, Some("evil.example:43210")));
        assert!(host_allowed(&auth, None));
    }

    #[test]
    fn host_check_requires_exact_loopback_host_and_port() {
        let auth = desktop_auth();
        assert!(host_allowed(&auth, Some("127.0.0.1:43210")));
        assert!(host_allowed(&auth, Some("LOCALHOST:43210")));
        assert!(!host_allowed(&auth, Some("127.0.0.1:5000")));
        assert!(!host_allowed(&auth, Some("localhost")));
        assert!(!host_allowed(&auth, Some("127.0.0.1.evil.example:43210")));
        assert!(!host_allowed(&auth, None));
    }

    #[tokio::test]
    async fn router_rejects_foreign_host_in_desktop_mode() {
        let app = desktop_app().await;
        assert_eq!(
            status(&app, get("evil.example:43210", "/api/auth/capabilities")).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&app, desktop_session("evil.example:43210", None)).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&app, get("127.0.0.1:43210", "/api/auth/capabilities")).await,
            StatusCode::OK
        );
        assert_eq!(
            status(&app, get("localhost:43210", "/api/auth/capabilities")).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn router_ignores_host_outside_desktop_mode() {
        let app = crate::app(crate::test_state().await);
        assert_eq!(
            status(&app, get("evil.example:43210", "/api/auth/capabilities")).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn desktop_session_rejects_foreign_origin() {
        let app = desktop_app().await;
        for origin in [
            "http://evil.example:43210",
            "https://127.0.0.1:43210",
            "http://127.0.0.1:5001",
            "null",
        ] {
            assert_eq!(
                status(&app, desktop_session("127.0.0.1:43210", Some(origin))).await,
                StatusCode::FORBIDDEN,
                "{origin}"
            );
        }
        // Past the guards the test state has no database, so the handler fails later.
        for origin in [
            Some("http://127.0.0.1:43210"),
            Some("http://localhost:43210"),
            None,
        ] {
            assert_ne!(
                status(&app, desktop_session("127.0.0.1:43210", origin)).await,
                StatusCode::FORBIDDEN,
                "{origin:?}"
            );
        }
    }
}
