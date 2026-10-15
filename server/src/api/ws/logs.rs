//! Live tail of `current.jsonl`. History is loaded with `GET /api/logs`.

use crate::logs::read::{self, FilterParams};
use crate::middleware::admin::AdminUser;
use crate::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const TAIL_INTERVAL: Duration = Duration::from_millis(500);

pub async fn logs_ws_handler(
    State(state): State<Arc<AppState>>,
    AdminUser(_admin): AdminUser,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let dir = PathBuf::from(&state.config.logs.dir);
    ws.on_upgrade(move |socket| handle_logs_socket(socket, dir))
}

async fn handle_logs_socket(socket: WebSocket, dir: PathBuf) {
    let (mut sender, mut receiver) = socket.split();
    let filter = loop {
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<FilterParams>(&text) {
                Ok(params) => match read::filter_from_params(&params) {
                    Ok(filter) => break filter,
                    Err(_) => return,
                },
                Err(_) => return,
            },
            Some(Ok(Message::Ping(payload))) => {
                if sender.send(Message::Pong(payload)).await.is_err() {
                    return;
                }
            }
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
            Some(Ok(_)) => {}
        }
    };

    let mut tail = crate::logs::tail::FileTail::start(&dir);
    let mut interval = tokio::time::interval(TAIL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            _ = interval.tick() => {
                let lines = match tail.poll_lines() {
                    Ok(lines) => lines,
                    Err(error) => {
                        tracing::warn!(error = %error, "log tail read failed");
                        continue;
                    }
                };
                for raw in lines {
                    let Some(line) = read::matching_jsonl_line(&filter, &raw) else {
                        continue;
                    };
                    let Ok(text) = serde_json::to_string(&line) else {
                        continue;
                    };
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}
