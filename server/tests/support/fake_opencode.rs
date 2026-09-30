//! Stand-in for `opencode serve` in integration tests: records how it was
//! launched and answers `GET /doc` until killed.

use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("serve") {
        eprintln!("fake-opencode: expected `serve`, got {args:?}");
        std::process::exit(2);
    }
    let hostname = flag(&args, "--hostname").unwrap_or_else(|| "127.0.0.1".into());
    let port = flag(&args, "--port").expect("--port is required");

    if let Ok(record) = std::env::var("FAKE_OPENCODE_RECORD") {
        let body = serde_json::json!({
            "opencodeConfig": std::env::var("OPENCODE_CONFIG").ok(),
            "hasToken": std::env::var("COPPICE_MCP_TOKEN").is_ok(),
            "pid": std::process::id(),
        });
        std::fs::write(record, body.to_string()).expect("write record");
    }

    let app = Router::new().route("/doc", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(format!("{hostname}:{port}"))
        .await
        .expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
