use crate::api::ws::auth::auth_user_from_cookie;
use crate::domain::run::run_status_to_str;
use crate::services::artifact_service::RunArtifactPaths;
use crate::services::run_service::RunService;
use crate::sessions::TerminalFrame;
use crate::AppState;
use time::OffsetDateTime;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use uuid::Uuid;

pub async fn live_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(run_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    auth_user_from_cookie(&state, cookie)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(ws.on_upgrade(move |socket| handle_live_socket(state, run_id, socket)))
}

async fn handle_live_socket(state: Arc<AppState>, run_id: Uuid, socket: WebSocket) {
    let (mut sender, _receiver) = socket.split();

    if let Some(handle) = state.run_streams.get(run_id) {
        for frame in handle.buffered_tail() {
            if send_frame(&mut sender, &frame).await.is_err() {
                return;
            }
        }

        let mut rx = handle.subscribe();
        loop {
            match rx.recv().await {
                Ok(frame) => {
                    if send_frame(&mut sender, &frame).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }

            if state.run_streams.get(run_id).is_none() {
                break;
            }
        }
    } else if let Some(log_bytes) = read_terminal_log_artifact(&state, run_id) {
        let frame = TerminalFrame {
            seq: 0,
            data: log_bytes,
            ts: OffsetDateTime::now_utc(),
        };
        if send_frame(&mut sender, &frame).await.is_err() {
            return;
        }
    }

    let status = if let Some(pool) = state.db.as_ref() {
        RunService::new(pool)
            .get(run_id)
            .await
            .ok()
            .map(|run| run_status_to_str(run.status).to_string())
            .unwrap_or_else(|| "unknown".into())
    } else {
        "unknown".into()
    };
    let end = TerminalFrame::end_message(&status);
    let _ = sender
        .send(Message::Text(serde_json::to_string(&end).unwrap_or_default().into()))
        .await;
}

fn read_terminal_log_artifact(state: &AppState, run_id: Uuid) -> Option<Vec<u8>> {
    let paths = RunArtifactPaths::new(&state.config.storage.artifacts_dir, &run_id.to_string());
    if !paths.terminal_log.is_file() {
        return None;
    }
    std::fs::read(&paths.terminal_log).ok().filter(|bytes| !bytes.is_empty())
}

async fn send_frame(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    frame: &TerminalFrame,
) -> Result<(), ()> {
    let json = frame.to_ws_json();
    sender
        .send(Message::Text(serde_json::to_string(&json).unwrap_or_default().into()))
        .await
        .map_err(|_| ())
}
