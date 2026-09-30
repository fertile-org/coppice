//! Dev probe: a standalone MCP endpoint used to verify how agent CLIs attach per-run MCP servers.
//!
//! Run: `PORT=5099 cargo run -p coppice-server --example mcp_probe` (`HOST=0.0.0.0` to reach it from a container)
//! Requires `Authorization: Bearer probe-token` on `POST /mcp`.

use axum::{
    extract::Request,
    http::{header, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use coppice_server::mcp::protocol::{handle_rpc, ToolDefinition, ToolHost, ToolOutput};
use serde_json::{json, Value};

const TOKEN: &str = "probe-token";

struct ProbeHost;

#[async_trait::async_trait]
impl ToolHost for ProbeHost {
    fn list(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "ping".into(),
            description: "Reply with pong and the given message.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "message": { "type": "string" } },
                "required": ["message"],
            }),
            read_only: true,
        }]
    }

    async fn call(&self, name: &str, args: Value) -> ToolOutput {
        if name != "ping" {
            return ToolOutput {
                text: format!("unknown tool {name}"),
                is_error: true,
            };
        }
        let message = args.get("message").and_then(Value::as_str).unwrap_or("");
        ToolOutput {
            text: format!("pong {message}"),
            is_error: false,
        }
    }
}

async fn log_and_auth(req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {TOKEN}"));
    eprintln!(
        "{} {} authorized={authorized}",
        req.method(),
        req.uri().path()
    );
    if req.method() == Method::POST && !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(req).await
}

async fn mcp_post(Json(body): Json<Value>) -> Response {
    eprintln!("  rpc: {body}");
    match handle_rpc(body, &ProbeHost).await {
        Some(res) => Json(res).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "5099".into());
    let app = Router::new()
        .route("/mcp", post(mcp_post))
        .route("/mcp", get(|| async { StatusCode::METHOD_NOT_ALLOWED }))
        .layer(middleware::from_fn(log_and_auth));
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    eprintln!("mcp_probe listening on http://{addr}/mcp (Bearer {TOKEN})");
    axum::serve(listener, app).await.expect("serve");
}
