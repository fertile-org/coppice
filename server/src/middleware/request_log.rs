//! One info line per API request: method, path (with query), status, duration.
//! Bodies, cookies, and auth headers are not read.

use std::time::Instant;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub async fn log_request(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_owned())
        .unwrap_or_else(|| request.uri().path().to_owned());
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
    async fn logs_method_path_query_status_and_duration_only() {
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
                    .uri("/api/connectors/cursor/model-providers/cursor/models?refresh=true")
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
            text.contains("/api/connectors/cursor/model-providers/cursor/models?refresh=true"),
            "{text}"
        );
        assert!(
            text.contains("status=200") || text.contains("status: 200"),
            "{text}"
        );
        assert!(text.contains("duration_ms"), "{text}");
        assert!(!text.contains("secret-cookie"), "{text}");
        assert!(!text.contains("secret-token"), "{text}");
        assert!(!text.contains("secret-body"), "{text}");
    }
}
