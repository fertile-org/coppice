#![cfg(feature = "embedded-test-db")]

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use coppice_server::mcp::protocol::{ToolContent, ToolResult};
use coppice_server::mcp::proxy::{
    HttpTransport, McpConnection, McpServerPool, McpTransport, PoolConfig, PoolError,
    PoolServerSpec, ProxyError, ServerHealth, ServerKey, StdioTransport, Transports,
};
use coppice_server::plugins::capability::{McpServerEntry, McpServerTransport};
use coppice_server::plugins::placeholders::ResolvedTransport;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn fake_mcp(env: &[(&str, &str)]) -> ResolvedTransport {
    ResolvedTransport::Stdio {
        command: env!("CARGO_BIN_EXE_fake-mcp").into(),
        args: Vec::new(),
        env: map(env),
        secrets: Vec::new(),
    }
}

async fn connect_stdio(env: &[(&str, &str)], cwd: &Path) -> Box<dyn McpConnection> {
    match StdioTransport.connect(&fake_mcp(env), cwd).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect failed: {e}"),
    }
}

fn text_of(result: &ToolResult) -> String {
    match result.content.as_slice() {
        [ToolContent::Text(text)] => text.clone(),
        other => panic!("expected one text block, got {other:?}"),
    }
}

async fn call_text(conn: &dyn McpConnection, name: &str, args: Value) -> String {
    let result = conn.call_tool(name, args).await.expect("call_tool");
    assert!(!result.is_error);
    text_of(&result)
}

#[tokio::test]
async fn stdio_lists_and_calls() {
    let dir = tempfile::tempdir().unwrap();
    let transport = Transports::builtin()
        .get("stdio")
        .expect("stdio registered");
    assert_eq!(transport.kind(), "stdio");
    let conn = match transport.connect(&fake_mcp(&[]), dir.path()).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect failed: {e}"),
    };

    let tools = conn.list_tools().await.expect("list_tools");
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["echo", "write_note", "env", "sleep", "crash"]);
    let echo = tools.iter().find(|t| t.name == "echo").unwrap();
    assert!(echo.read_only);
    assert!(!echo.description.is_empty());
    assert_eq!(echo.input_schema["type"], "object");
    assert!(
        !tools
            .iter()
            .find(|t| t.name == "write_note")
            .unwrap()
            .read_only
    );

    let result = conn
        .call_tool("echo", json!({ "text": "hi" }))
        .await
        .expect("call_tool");
    assert_eq!(result.content, vec![ToolContent::Text("hi".into())]);
    assert!(!result.is_error);
    assert!(!conn.is_closed());
    conn.close().await;
}

#[tokio::test]
async fn stdio_env_is_minimal() {
    std::env::set_var("SOME_SERVER_SECRET", "x");
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[("GREETING", "hey")], dir.path()).await;
    let env = |name: &str| json!({ "name": name });
    assert_eq!(
        call_text(&*conn, "env", env("SOME_SERVER_SECRET")).await,
        "<unset>"
    );
    assert_ne!(call_text(&*conn, "env", env("PATH")).await, "<unset>");
    assert_eq!(call_text(&*conn, "env", env("GREETING")).await, "hey");
    conn.close().await;
}

#[tokio::test]
async fn stdio_tool_output_redacts_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let spec = ResolvedTransport::Stdio {
        command: env!("CARGO_BIN_EXE_fake-mcp").into(),
        args: Vec::new(),
        env: map(&[("LEAK", "--token=long-s3cr3t"), ("SHORT", "ab")]),
        secrets: vec!["long-s3cr3t".into(), "ab".into()],
    };
    let conn = match StdioTransport.connect(&spec, dir.path()).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect failed: {e}"),
    };
    for error in [false, true] {
        let args = json!({ "name": "LEAK", "wrap": true, "error": error });
        let result = conn.call_tool("env", args).await.expect("call_tool");
        assert_eq!(result.is_error, error);
        assert_eq!(text_of(&result), "token=--token=[redacted]; done");
    }
    // Values shorter than the output minimum are left alone.
    assert_eq!(
        call_text(&*conn, "env", json!({ "name": "SHORT" })).await,
        "ab"
    );
    conn.close().await;
}

#[tokio::test]
async fn stdio_survives_non_utf8_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[("FAKE_MCP_STDERR_GARBAGE", "1")], dir.path()).await;
    for text in ["one", "two"] {
        assert_eq!(
            call_text(&*conn, "echo", json!({ "text": text })).await,
            text
        );
        assert!(!conn.is_closed());
    }
    conn.close().await;
}

#[tokio::test]
async fn stdio_cwd_is_plugin_root() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[("FAKE_MCP_PID_FILE", "pid.txt")], dir.path()).await;
    assert!(dir.path().join("pid.txt").exists());
    conn.close().await;
}

#[tokio::test]
async fn stdio_crash_marks_closed() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[], dir.path()).await;
    let result = conn.call_tool("crash", json!({})).await;
    assert!(
        matches!(result, Err(ProxyError::Closed | ProxyError::Protocol(_))),
        "got {result:?}"
    );
    assert!(conn.is_closed());
}

#[tokio::test]
async fn stdio_start_failure_is_start_error() {
    let dir = tempfile::tempdir().unwrap();
    match StdioTransport
        .connect(&fake_mcp(&[("FAKE_MCP_START_FAIL", "1")]), dir.path())
        .await
    {
        Err(ProxyError::Start(_)) => {}
        Err(other) => panic!("expected Start, got {other:?}"),
        Ok(_) => panic!("expected Start error"),
    }

    let missing = ResolvedTransport::Stdio {
        command: dir
            .path()
            .join("no-such-bin")
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
        env: BTreeMap::new(),
        secrets: Vec::new(),
    };
    assert!(matches!(
        StdioTransport.connect(&missing, dir.path()).await,
        Err(ProxyError::Start(_))
    ));
}

#[tokio::test]
async fn stdio_tools_changed_flag() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[("FAKE_MCP_TOOLS_CHANGED", "1")], dir.path()).await;
    assert!(!conn.take_tools_changed());
    assert_eq!(call_text(&*conn, "echo", json!({ "text": "a" })).await, "a");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !conn.take_tools_changed() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "tools changed flag not set"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let tools = conn.list_tools().await.expect("list_tools");
    assert!(tools.iter().any(|t| t.name == "extra"));
    assert!(!conn.take_tools_changed());
    conn.close().await;
}

#[tokio::test]
async fn close_kills_child() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_stdio(&[("FAKE_MCP_PID_FILE", "pid.txt")], dir.path()).await;
    let pid = std::fs::read_to_string(dir.path().join("pid.txt")).unwrap();
    let proc_path = Path::new("/proc").join(pid.trim());
    assert!(proc_path.exists());
    conn.close().await;
    assert!(conn.is_closed());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while proc_path.exists() {
        assert!(tokio::time::Instant::now() < deadline, "child still alive");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[derive(Clone)]
struct Stub {
    status: Option<StatusCode>,
}

async fn stub_post(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Json(msg): Json<Value>,
) -> Response {
    if let Some(status) = stub.status {
        return status.into_response();
    }
    let authorized =
        headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer h-val");
    if !authorized {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(id) = msg.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match msg["method"].as_str() {
        Some("initialize") => json!({
            "protocolVersion": msg["params"]["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "stub", "version": "0" },
        }),
        Some("tools/list") => json!({ "tools": [{
            "name": "remote",
            "description": "Remote tool",
            "inputSchema": { "type": "object" },
        }] }),
        Some("tools/call") => json!({
            "content": [{ "type": "text", "text": format!("called {}", msg["params"]["name"]) }],
        }),
        _ => {
            return Json(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": "method not found" },
            }))
            .into_response()
        }
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

async fn spawn_stub(status: Option<StatusCode>) -> String {
    let app = Router::new()
        .route(
            "/mcp",
            post(stub_post)
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .delete(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .with_state(Stub { status });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("127.0.0.1:{}", addr.port())
}

#[tokio::test]
async fn http_lists_and_calls_with_headers() {
    let host = spawn_stub(None).await;
    let spec = ResolvedTransport::Http {
        url: format!("http://{host}/mcp"),
        headers: map(&[("Authorization", "Bearer h-val")]),
        secrets: Vec::new(),
    };
    let dir = tempfile::tempdir().unwrap();
    let conn = match HttpTransport.connect(&spec, dir.path()).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect failed: {e}"),
    };
    let tools = conn.list_tools().await.expect("list_tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "remote");
    assert!(!tools[0].read_only);
    assert_eq!(
        call_text(&*conn, "remote", json!({})).await,
        "called \"remote\""
    );
    conn.close().await;
}

#[tokio::test]
async fn http_error_hides_header_value() {
    let host = spawn_stub(Some(StatusCode::UNAUTHORIZED)).await;
    let spec = ResolvedTransport::Http {
        url: format!("http://user:u-pass@{host}/mcp?token=q-secret"),
        headers: map(&[("Authorization", "Bearer h-val")]),
        secrets: Vec::new(),
    };
    assert_error_hides_secrets(&spec).await;

    let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = unused.local_addr().unwrap().port();
    drop(unused);
    let refused = ResolvedTransport::Http {
        url: format!("http://user:u-pass@127.0.0.1:{port}/mcp?token=q-secret"),
        headers: map(&[("Authorization", "Bearer h-val")]),
        secrets: Vec::new(),
    };
    assert_error_hides_secrets(&refused).await;

    let path_secret = format!("http://127.0.0.1:{port}/hooks/p4th-s3cr3t/mcp");
    let refused = ResolvedTransport::Http {
        url: path_secret.clone(),
        headers: map(&[("Authorization", "Bearer h-val")]),
        secrets: Vec::new(),
    };
    let message = assert_error_hides_secrets(&refused).await;
    for leaked in ["p4th-s3cr3t", path_secret.as_str(), "/hooks/"] {
        assert!(!message.contains(leaked), "{leaked} leaked in {message}");
    }
}

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl LogCapture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// Every event on this thread, at every level, from every target (rmcp included).
fn capture_logs() -> (LogCapture, tracing::subscriber::DefaultGuard) {
    let logs = LogCapture::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    (logs, tracing::subscriber::set_default(subscriber))
}

async fn unused_port() -> u16 {
    let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    unused.local_addr().unwrap().port()
}

/// Accepts any path, hands out a session on `initialize`, and has no SSE stream.
async fn session_stub(method: Method, body: Bytes) -> Response {
    if method == Method::DELETE {
        return StatusCode::OK.into_response();
    }
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let msg: Value = serde_json::from_slice(&body).unwrap_or_default();
    let Some(id) = msg.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match msg["method"].as_str() {
        Some("initialize") => json!({
            "protocolVersion": msg["params"]["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "stub", "version": "0" },
        }),
        _ => json!({ "tools": [] }),
    };
    (
        [("mcp-session-id", "s-1")],
        Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
    )
        .into_response()
}

#[tokio::test]
async fn http_transport_logs_never_show_the_url() {
    let (logs, _guard) = capture_logs();
    let dir = tempfile::tempdir().unwrap();
    let secret_url = |port: u16| {
        format!("http://user:u-pass@127.0.0.1:{port}/hooks/p4th-s3cr3t/mcp?token=q-secret")
    };

    let refused = ResolvedTransport::Http {
        url: secret_url(unused_port().await),
        headers: map(&[("Authorization", "Bearer h-val")]),
        secrets: Vec::new(),
    };
    assert!(HttpTransport.connect(&refused, dir.path()).await.is_err());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().fallback(session_stub))
            .await
            .unwrap()
    });
    let live = ResolvedTransport::Http {
        url: secret_url(port),
        headers: BTreeMap::new(),
        secrets: Vec::new(),
    };
    let conn = match HttpTransport.connect(&live, dir.path()).await {
        Ok(conn) => conn,
        Err(e) => panic!("connect failed: {e}"),
    };
    assert!(conn.list_tools().await.unwrap().is_empty());
    server.abort();
    let _ = server.await;
    // Session cleanup (DELETE) now hits a closed port and rmcp logs the failure.
    conn.close().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let text = logs.text();
    assert!(
        text.contains("fail to delete session"),
        "expected rmcp's cleanup failure to be logged:\n{text}"
    );
    for leaked in ["u-pass", "q-secret", "p4th-s3cr3t", "for url ("] {
        assert!(!text.contains(leaked), "{leaked} leaked in logs:\n{text}");
    }
}

async fn assert_error_hides_secrets(spec: &ResolvedTransport) -> String {
    let dir = tempfile::tempdir().unwrap();
    let message = match HttpTransport.connect(spec, dir.path()).await {
        Err(e) => e.to_string(),
        Ok(conn) => match conn.list_tools().await {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected an error"),
        },
    };
    for secret in ["h-val", "u-pass", "q-secret", "user:"] {
        assert!(!message.contains(secret), "{secret} leaked in {message}");
    }
    message
}

fn pool_spec(root: &Path) -> PoolServerSpec {
    PoolServerSpec {
        key: ServerKey {
            plugin_id: Uuid::new_v4(),
            server: "fake".into(),
        },
        plugin_name: "demo".into(),
        plugin_root: root.to_path_buf(),
        entry: McpServerEntry {
            name: "fake".into(),
            transport: McpServerTransport::Stdio {
                command: env!("CARGO_BIN_EXE_fake-mcp").into(),
                args: Vec::new(),
                env: map(&[("FAKE_MCP_PID_FILE", "${CLAUDE_PLUGIN_ROOT}/pid.txt")]),
            },
            error: None,
        },
        settings: BTreeMap::new(),
    }
}

fn test_pool() -> McpServerPool {
    McpServerPool::new(
        Transports::builtin(),
        PoolConfig {
            start_timeout: Duration::from_secs(20),
            idle_shutdown: Duration::from_secs(600),
            backoff_initial: Duration::from_millis(200),
            backoff_max: Duration::from_secs(1),
            unhealthy_after: 3,
        },
    )
}

async fn pool_echo(pool: &McpServerPool, spec: &PoolServerSpec, text: &str) -> String {
    match pool.call(spec, "echo", json!({ "text": text })).await {
        Ok(result) => text_of(&result),
        Err(e) => panic!("pool call failed: {e}"),
    }
}

fn read_pid(root: &Path) -> String {
    std::fs::read_to_string(root.join("pid.txt")).expect("pid file")
}

#[tokio::test]
async fn pool_shares_one_process_across_callers() {
    let dir = tempfile::tempdir().unwrap();
    let pool = test_pool();
    let spec = pool_spec(dir.path());
    let (a, b) = tokio::join!(pool_echo(&pool, &spec, "a"), pool_echo(&pool, &spec, "b"));
    assert_eq!((a.as_str(), b.as_str()), ("a", "b"));
    let pid = read_pid(dir.path());
    assert_eq!(pool_echo(&pool, &spec, "c").await, "c");
    assert_eq!(read_pid(dir.path()), pid);
    assert_eq!(pool.health(&spec.key), ServerHealth::Ready);
    pool.stop_plugin(spec.key.plugin_id).await;
    assert_eq!(pool.health(&spec.key), ServerHealth::Stopped);
}

#[tokio::test]
async fn pool_health_reflects_server_killed_while_idle() {
    let dir = tempfile::tempdir().unwrap();
    let pool = test_pool();
    let spec = pool_spec(dir.path());
    assert_eq!(pool_echo(&pool, &spec, "a").await, "a");
    assert_eq!(pool.health(&spec.key), ServerHealth::Ready);

    let killed = std::process::Command::new("kill")
        .args(["-9", read_pid(dir.path()).trim()])
        .status()
        .unwrap();
    assert!(killed.success());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while pool.health(&spec.key) == ServerHealth::Ready {
        assert!(
            tokio::time::Instant::now() < deadline,
            "killed server still reported ready"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(pool.health(&spec.key), ServerHealth::Backoff);
    assert_eq!(
        pool.last_error(&spec.key).as_deref(),
        Some("server exited or closed the connection")
    );
}

#[tokio::test]
async fn pool_restarts_crashed_stdio_server() {
    let dir = tempfile::tempdir().unwrap();
    let pool = test_pool();
    let spec = pool_spec(dir.path());
    assert_eq!(pool_echo(&pool, &spec, "a").await, "a");
    let first = read_pid(dir.path());

    assert_eq!(
        pool.call(&spec, "crash", json!({})).await.err(),
        Some(PoolError::Unavailable)
    );
    assert_eq!(pool.health(&spec.key), ServerHealth::Backoff);
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(pool_echo(&pool, &spec, "b").await, "b");
    assert_ne!(read_pid(dir.path()), first);
    pool.stop_plugin(spec.key.plugin_id).await;
}
