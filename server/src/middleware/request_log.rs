//! One info line per HTTP request: method, path, status, duration.
//! Bodies and cookies are not logged. 404s are included.

use std::time::Instant;

use axum::{body::Body, http::Request, middleware::Next, response::Response};

pub async fn log_request(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let duration_ms = started.elapsed().as_millis() as u64;
    tracing::info!(method, path, status, duration_ms, "request");
    response
}
