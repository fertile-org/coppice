#![cfg(feature = "embedded-test-db")]

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use coppice_server::mcp::protocol::{ToolContent, ToolResult};
use coppice_server::mcp::proxy::{
    HttpTransport, McpConnection, McpTransport, ProxyError, StdioTransport, Transports,
};
use coppice_server::plugins::placeholders::ResolvedTransport;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

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
    };
    assert_error_hides_secrets(&spec).await;

    let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = unused.local_addr().unwrap().port();
    drop(unused);
    let refused = ResolvedTransport::Http {
        url: format!("http://user:u-pass@127.0.0.1:{port}/mcp?token=q-secret"),
        headers: map(&[("Authorization", "Bearer h-val")]),
    };
    assert_error_hides_secrets(&refused).await;
}

async fn assert_error_hides_secrets(spec: &ResolvedTransport) {
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
}
