//! One info line per API request: method, path, status, duration.
//! The query string is omitted. Bodies, cookies, and auth headers are not read.

use std::time::Instant;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub async fn log_request(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        %method,
        %path,
        status = response.status().as_u16(),
        duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "http request"
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logs_method_path_status_and_duration_without_the_query() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = Arc::clone(&buf);
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .without_time()
            .with_writer(move || LogWriter(Arc::clone(&writer)))
            .finish();
        let app = Router::new()
            .route(
                "/api/connectors/cursor/model-providers/cursor/models",
                get(|| async { "ok" }),
            )
            .layer(axum::middleware::from_fn(log_request));
        let _guard = tracing::subscriber::set_default(subscriber);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/connectors/cursor/model-providers/cursor/models?refresh=query-secret-value")
                    .header("cookie", "coppice_session=secret-cookie")
                    .header("authorization", "Bearer secret-token")
                    .body(Body::from("secret-body"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        drop(_guard);
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(text.contains("GET"), "{text}");
        assert!(
            text.contains("/api/connectors/cursor/model-providers/cursor/models"),
            "{text}"
        );
        assert!(!text.contains('?'), "{text}");
        assert!(!text.contains("query-secret-value"), "{text}");
        assert!(
            text.contains("status=200") || text.contains("status: 200"),
            "{text}"
        );
        assert!(text.contains("duration_ms"), "{text}");
        assert!(!text.contains("secret-cookie"), "{text}");
        assert!(!text.contains("secret-token"), "{text}");
        assert!(!text.contains("secret-body"), "{text}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logs_an_unmatched_path_as_404_without_the_query() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = Arc::clone(&buf);
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .without_time()
            .with_writer(move || LogWriter(Arc::clone(&writer)))
            .finish();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>app</html>").unwrap();
        let app = Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .fallback(axum::http::StatusCode::NOT_FOUND)
            .layer(axum::middleware::from_fn(log_request));
        // Desktop wraps this router. The missing /api path still has to be logged.
        let app = crate::static_web::with_static_web(app, dir.path());
        let _guard = tracing::subscriber::set_default(subscriber);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/this-route-is-missing?refresh=query-secret-value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        let matched = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(matched.status(), axum::http::StatusCode::OK);
        let page = app
            .oneshot(
                Request::builder()
                    .uri("/boards/123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), axum::http::StatusCode::OK);
        drop(_guard);
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        let logged: Vec<_> = text
            .lines()
            .filter(|line| line.contains("http request"))
            .collect();
        assert_eq!(logged.len(), 2, "{text}");
        let line = logged
            .iter()
            .find(|line| line.contains("/api/this-route-is-missing"))
            .copied()
            .unwrap_or_else(|| panic!("404 was not logged: {text}"));
        assert!(line.contains("GET"), "{line}");
        assert!(!line.contains('?'), "{line}");
        assert!(!line.contains("query-secret-value"), "{line}");
        assert!(
            line.contains("status=404") || line.contains("status: 404"),
            "{line}"
        );
        assert!(line.contains("duration_ms"), "{line}");
        assert!(
            logged.iter().any(|line| {
                line.contains("/api/health")
                    && (line.contains("status=200") || line.contains("status: 200"))
            }),
            "{text}"
        );
        assert!(!text.contains("/boards/123"), "{text}");
    }
}
