use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, Request, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use clap::Args;
use coppice_config::AppConfig;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Args)]
pub struct WebStartArgs {}

struct WebState {
    api_url: String,
    client: reqwest::Client,
}

pub async fn run(_args: WebStartArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    let static_dir = resolve_static_dir(&config)?;
    let api_url = config.web_api_url();
    let port = config.web.port;

    if !static_dir.is_dir() {
        anyhow::bail!(
            "web static dir not found: {} (build the SPA or set [web].static_dir)",
            static_dir.display()
        );
    }

    let state = Arc::new(WebState {
        api_url: api_url.clone(),
        client: reqwest::Client::new(),
    });

    let index = static_dir.join("index.html");
    let spa = ServeDir::new(&static_dir).not_found_service(ServeFile::new(index));

    let app = Router::new()
        .route("/health", any(proxy))
        .nest("/api", Router::new().fallback(any(proxy)))
        .with_state(state)
        .fallback_service(spa);

    let addr = format!("0.0.0.0:{port}");
    println!(
        "serving web at http://127.0.0.1:{port} (static: {}, api: {api_url})",
        static_dir.display()
    );

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn proxy(
    State(state): State<Arc<WebState>>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let url = format!(
        "{}{}",
        state.api_url.trim_end_matches('/'),
        path_and_query
    );

    let method = req.method().clone();
    let (parts, body) = req.into_parts();
    const MAX_PROXY_BODY: usize = 10 * 1024 * 1024;
    let body_bytes = axum::body::to_bytes(body, MAX_PROXY_BODY)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut builder = state.client.request(method, &url).body(body_bytes);
    for (name, value) in parts.headers.iter() {
        if forward_request_header(name) {
            builder = builder.header(name, value);
        }
    }

    let upstream = builder
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = upstream.headers().clone();
    let bytes = upstream
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut response = Response::builder().status(status);
    for (name, value) in upstream_headers.iter() {
        if forward_response_header(name) {
            response = response.header(name, value);
        }
    }

    response
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn forward_request_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "accept" | "accept-language" | "authorization" | "content-type" | "cookie" | "x-csrf-token"
    )
}

fn forward_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "content-type" | "content-length" | "set-cookie" | "cache-control"
    )
}

fn resolve_static_dir(config: &AppConfig) -> anyhow::Result<PathBuf> {
    let configured = PathBuf::from(&config.web.static_dir);
    if configured.is_dir() {
        return Ok(configured);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let bundled = parent.join("web/dist");
            if bundled.is_dir() {
                return Ok(bundled);
            }
        }
    }

    Ok(configured)
}
