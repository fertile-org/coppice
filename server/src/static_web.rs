use std::path::Path;

use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeader;

/// Paths owned by the server; unmatched requests under them must 404 rather
/// than load the SPA.
const SERVER_PREFIXES: &[&str] = &["/api", "/mcp", "/ws", "/health"];

fn is_server_path(path: &str) -> bool {
    SERVER_PREFIXES.iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Serves the built SPA from `web_dir` for every route `router` does not
/// match; unknown client routes get `index.html` so deep links work.
pub fn with_static_web(router: Router, web_dir: &Path) -> Router {
    let index = SetResponseHeader::overriding(
        ServeFile::new(web_dir.join("index.html")),
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    let files = ServeDir::new(web_dir)
        .append_index_html_on_directories(false)
        .fallback(index.clone());
    router.fallback(move |req: Request| async move {
        let path = req.uri().path();
        if is_server_path(path) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let res = if path == "/" || path == "/index.html" {
            index.oneshot(req).await
        } else {
            files.oneshot(req).await
        };
        match res {
            Ok(res) => res.into_response(),
            Err(never) => match never {},
        }
    })
}

#[cfg(test)]
mod tests {
    use super::with_static_web;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::Router;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn web_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>app</html>").unwrap();
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/a.js"), "console.log(1)").unwrap();
        dir
    }

    async fn get(path: &str) -> (StatusCode, Option<String>, String) {
        let dir = web_dir();
        let res = with_static_web(Router::new(), dir.path())
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let cache = res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap().to_string());
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (status, cache, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn root_serves_index_without_caching() {
        let (status, cache, body) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("app"));
        assert_eq!(cache.as_deref(), Some("no-cache"));
    }

    #[tokio::test]
    async fn client_routes_fall_back_to_index() {
        let (status, cache, body) = get("/boards/123").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("app"));
        assert_eq!(cache.as_deref(), Some("no-cache"));
    }

    #[tokio::test]
    async fn assets_are_served_from_disk() {
        let (status, _, body) = get("/assets/a.js").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "console.log(1)");
    }

    #[tokio::test]
    async fn server_prefixes_are_not_rewritten_to_index() {
        for path in ["/api/nope", "/mcp/nope", "/ws/nope", "/health/nope", "/api"] {
            let (status, _, _) = get(path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
    }
}
