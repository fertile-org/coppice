use crate::api::ws::auth::auth_user_from_cookie;
use crate::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

pub async fn events_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    auth_user_from_cookie(&state, cookie)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(ws.on_upgrade(move |socket| handle_events_socket(state, socket)))
}

async fn handle_events_socket(state: Arc<AppState>, socket: WebSocket) {
    let (mut sender, _receiver) = socket.split();
    let mut rx = state.event_bus.subscribe();

    loop {
        match rx.recv().await {
            Ok(event) => {
                let Ok(raw) = serde_json::to_string(&event) else {
                    continue;
                };
                if sender.send(Message::Text(raw.into())).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
