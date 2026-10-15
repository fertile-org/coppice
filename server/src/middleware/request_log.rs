//! One info line per API request: method, path, status, duration, request id.
//! The query string is omitted. Bodies, cookies, and auth headers are not read.
//! `run_id` is set when the path names an agent run.

use std::time::Instant;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub async fn log_request(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let request_id = uuid::Uuid::new_v4();
    let run_id = run_id_from_path(&path);
    let started = Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        source = "http",
        %method,
        %path,
        status = response.status().as_u16(),
        duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        %request_id,
        run_id,
        "http request"
    );
    response
}

/// `/api/agent-runs/{uuid}` and `/ws/agent-runs/{uuid}`, including subpaths.
/// `active` and any non-uuid segment are not a run id.
fn run_id_from_path(path: &str) -> Option<&str> {
    let rest = path
        .strip_prefix("/api/agent-runs/")
        .or_else(|| path.strip_prefix("/ws/agent-runs/"))?;
    let id = rest.split('/').next().filter(|id| !id.is_empty())?;
    uuid::Uuid::parse_str(id).ok()?;
    Some(id)
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

    #[test]
    fn server_http_and_cli_lines_share_one_daily_jsonl_file() {
        let dir = tempfile::tempdir().unwrap();
        crate::logs::with_cli_logs(dir.path(), || {
            crate::logs::log_cli_line(Some("run-cli"), None, "stderr", "from-cli");
            tracing::info!(run_id = "run-77", "scheduler tick");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let app = Router::new()
                    .route("/api/health", get(|| async { "ok" }))
                    .layer(axum::middleware::from_fn(log_request));
                let response = app
                    .oneshot(
                        Request::builder()
                            .uri("/api/health")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::OK);
            });
        })
        .unwrap();

        let files = daily_jsonl_files(dir.path());
        assert_eq!(files.len(), 1, "{files:?}");
        let name = files[0].file_name().unwrap().to_str().unwrap();
        let date = name
            .strip_prefix("coppice.")
            .and_then(|rest| rest.strip_suffix(".jsonl"))
            .unwrap_or_else(|| panic!("daily file name: {name}"));
        assert_eq!(date.len(), 10, "{name}");
        assert!(
            date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-',
            "{name}"
        );

        let lines = jsonl_values(&files);
        let cli = find_message(&lines, "from-cli");
        assert_eq!(cli["fields"]["source"], "cli");

        let server = find_message(&lines, "scheduler tick");
        assert_eq!(server["fields"]["source"], "server");
        assert_eq!(server["fields"]["run_id"], "run-77");

        let http = find_message(&lines, "http request");
        assert_eq!(http["fields"]["source"], "http");
        assert_eq!(http["fields"]["method"], "GET");
        assert_eq!(http["fields"]["path"], "/api/health");
        assert_eq!(http["fields"]["status"], 200);
        assert!(http["fields"]["duration_ms"].as_u64().is_some(), "{http}");
        let request_id = http["fields"]["request_id"].as_str().expect("request id");
        uuid::Uuid::parse_str(request_id).expect("request id is a uuid");
        assert!(http["fields"].get("run_id").is_none(), "{http}");
        let timestamp = http["timestamp"].as_str().expect("timestamp");
        assert!(
            timestamp.contains('T') && timestamp.ends_with('Z'),
            "{timestamp}"
        );
    }

    #[test]
    fn http_line_records_the_run_id_when_the_path_has_one() {
        let run = "11111111-1111-1111-1111-111111111111";
        let ws_run = "22222222-2222-2222-2222-222222222222";
        let dir = tempfile::tempdir().unwrap();
        crate::logs::with_cli_logs(dir.path(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let app = Router::new()
                    .route("/api/agent-runs/active", get(|| async { "ok" }))
                    .route(
                        "/api/agent-runs/{run_id}/stop",
                        axum::routing::post(|| async { "ok" }),
                    )
                    .route("/ws/agent-runs/{run_id}/live", get(|| async { "ok" }))
                    .layer(axum::middleware::from_fn(log_request));
                let requests = [
                    Request::builder()
                        .uri("/api/agent-runs/active")
                        .body(Body::empty())
                        .unwrap(),
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/agent-runs/{run}/stop"))
                        .body(Body::empty())
                        .unwrap(),
                    Request::builder()
                        .uri(format!("/ws/agent-runs/{ws_run}/live"))
                        .body(Body::empty())
                        .unwrap(),
                ];
                for request in requests {
                    let response = app.clone().oneshot(request).await.unwrap();
                    assert_eq!(response.status(), axum::http::StatusCode::OK);
                }
            });
        })
        .unwrap();

        let lines = jsonl_values(&daily_jsonl_files(dir.path()));
        let active = find_path(&lines, "/api/agent-runs/active");
        assert!(active["fields"].get("run_id").is_none(), "{active}");
        assert_eq!(
            find_path(&lines, &format!("/api/agent-runs/{run}/stop"))["fields"]["run_id"],
            run
        );
        assert_eq!(
            find_path(&lines, &format!("/ws/agent-runs/{ws_run}/live"))["fields"]["run_id"],
            ws_run
        );
        let ids: Vec<_> = lines
            .iter()
            .filter(|line| line["fields"]["message"] == "http request")
            .map(|line| line["fields"]["request_id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids.len(), 3, "{lines:?}");
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            3
        );
    }

    #[test]
    fn http_line_omits_query_body_and_auth_headers() {
        let dir = tempfile::tempdir().unwrap();
        crate::logs::with_cli_logs(dir.path(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let app = Router::new()
                    .route("/api/health", get(|| async { "ok" }))
                    .fallback(axum::http::StatusCode::NOT_FOUND)
                    .layer(axum::middleware::from_fn(log_request));
                let response = app
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/missing?refresh=query-secret-value")
                            .header("cookie", "coppice_session=secret-cookie")
                            .header("authorization", "Bearer secret-token")
                            .body(Body::from("secret-body"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
            });
        })
        .unwrap();

        let lines = jsonl_values(&daily_jsonl_files(dir.path()));
        let http = find_path(&lines, "/api/missing");
        assert_eq!(http["fields"]["method"], "POST");
        assert_eq!(http["fields"]["status"], 404);
        assert!(http["fields"].get("run_id").is_none(), "{http}");
        let text = std::fs::read_to_string(&daily_jsonl_files(dir.path())[0]).unwrap();
        assert!(!text.contains('?'), "{text}");
        assert!(!text.contains("query-secret-value"), "{text}");
        assert!(!text.contains("secret-cookie"), "{text}");
        assert!(!text.contains("secret-token"), "{text}");
        assert!(!text.contains("secret-body"), "{text}");
        assert!(!text.contains("authorization"), "{text}");
        assert!(!text.contains("cookie"), "{text}");
    }

    #[test]
    fn http_line_masks_a_registered_secret_in_the_path() {
        let token = "coppice-mask-7kQ9mP2x";
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::logs::SecretGuard::register([("COPPICE_MCP_TOKEN", token)]);
        crate::logs::with_cli_logs(dir.path(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let app = Router::new()
                    .fallback(axum::http::StatusCode::NOT_FOUND)
                    .layer(axum::middleware::from_fn(log_request));
                let response = app
                    .oneshot(
                        Request::builder()
                            .uri(format!("/api/{token}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
            });
        })
        .unwrap();

        let files = daily_jsonl_files(dir.path());
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(!text.contains(token), "{text}");
        assert!(text.contains("\u{2039}COPPICE_MCP_TOKEN\u{203A}"), "{text}");
        assert_eq!(
            find_message(&jsonl_values(&files), "http request")["fields"]["source"],
            "http"
        );
    }

    fn daily_jsonl_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                name.starts_with("coppice.") && name.ends_with(".jsonl")
            })
            .collect();
        files.sort();
        files
    }

    fn jsonl_values(files: &[std::path::PathBuf]) -> Vec<serde_json::Value> {
        let mut lines = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(path).unwrap();
            for line in text.lines() {
                lines
                    .push(serde_json::from_str(line).unwrap_or_else(|err| panic!("{err}: {line}")));
            }
        }
        lines
    }

    fn find_message<'a>(lines: &'a [serde_json::Value], message: &str) -> &'a serde_json::Value {
        lines
            .iter()
            .find(|line| line["fields"]["message"] == message)
            .unwrap_or_else(|| panic!("missing {message} in {lines:?}"))
    }

    fn find_path<'a>(lines: &'a [serde_json::Value], path: &str) -> &'a serde_json::Value {
        lines
            .iter()
            .find(|line| line["fields"]["path"] == path)
            .unwrap_or_else(|| panic!("missing {path} in {lines:?}"))
    }
}
