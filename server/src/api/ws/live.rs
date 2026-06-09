use crate::api::ws::auth::auth_user_from_cookie;
use crate::domain::run::{run_status_to_str, AgentRun, RunStatus};
use crate::services::artifact_service::{ArtifactService, RunArtifactPaths};
use crate::services::run_service::{RunError, RunService};
use crate::sessions::opencode_client::OpenCodeClient;
use crate::sessions::LiveMessage;
use crate::AppState;
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

struct RecoveryOutcome {
    recoverable: Option<bool>,
    reason: Option<String>,
}

async fn handle_live_socket(state: Arc<AppState>, run_id: Uuid, socket: WebSocket) {
    let (mut sender, _receiver) = socket.split();

    let Some(pool) = state.db.as_ref() else {
        let _ = send_live_message(
            &mut sender,
            &LiveMessage::End {
                status: "unknown".into(),
                reason: None,
                recoverable: false,
            },
        )
        .await;
        return;
    };

    let run_svc = RunService::new(pool);
    let run = match run_svc.get(run_id).await {
        Ok(run) => run,
        Err(RunError::NotFound) => return,
        Err(_) => {
            let _ = send_live_message(
                &mut sender,
                &LiveMessage::End {
                    status: "unknown".into(),
                    reason: None,
                    recoverable: false,
                },
            )
            .await;
            return;
        }
    };

    let connector = run_svc
        .agent_connector_for_run(run.agent_id)
        .await
        .ok()
        .flatten();
    let is_opencode = connector.as_deref() == Some("opencode");

    let recovery = if let Some(handle) = state.run_streams.get(run_id) {
        replay_and_subscribe(&state, run_id, &mut sender, &handle).await;
        None
    } else if is_opencode {
        Some(
            handle_opencode_recovery(&state, &mut sender, run_id, &run, &run_svc).await,
        )
    } else if let Some(log_bytes) = read_terminal_log_artifact(&state, run_id) {
        let msg = LiveMessage::Frame {
            seq: 0,
            data: log_bytes,
        };
        if send_live_message(&mut sender, &msg).await.is_err() {
            return;
        }
        None
    } else {
        None
    };

    let run = run_svc.get(run_id).await.unwrap_or(run);
    let status = run_status_to_str(run.status).to_string();
    let is_terminal = is_terminal_run_status(&status);
    let (recoverable, reason) = match recovery {
        Some(outcome) => (
            outcome.recoverable.unwrap_or(!is_terminal),
            outcome.reason,
        ),
        None => (is_active_run_status(run.status) && !is_terminal, None),
    };

    let end = LiveMessage::End {
        status,
        reason,
        recoverable,
    };
    let _ = send_live_message(&mut sender, &end).await;
}

async fn replay_and_subscribe(
    state: &AppState,
    run_id: Uuid,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    handle: &Arc<crate::sessions::run_registry::RunStreamHandle>,
) {
    for msg in handle.buffered_tail() {
        if send_live_message(sender, &msg).await.is_err() {
            return;
        }
    }

    let mut rx = handle.subscribe();
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if send_live_message(sender, &msg).await.is_err() {
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
}

async fn handle_opencode_recovery(
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    run_id: Uuid,
    run: &AgentRun,
    run_svc: &RunService<'_>,
) -> RecoveryOutcome {
    let paths = RunArtifactPaths::new(&state.config.storage.artifacts_dir, &run_id.to_string());

    if let Some(snapshot) = ArtifactService::read_session_snapshot(&paths) {
        if send_live_message(sender, &LiveMessage::Snapshot { snapshot })
            .await
            .is_err()
        {
            return RecoveryOutcome {
                recoverable: None,
                reason: None,
            };
        }
    }

    if !is_active_run_status(run.status) {
        return RecoveryOutcome {
            recoverable: Some(false),
            reason: None,
        };
    }

    let (session_id, worktree_path) = match (&run.session_id, &run.worktree_path) {
        (Some(session_id), Some(worktree_path)) => (session_id.clone(), worktree_path.clone()),
        _ => {
            return RecoveryOutcome {
                recoverable: Some(true),
                reason: None,
            };
        }
    };

    let Some(serve) = state.opencode_serve.as_ref() else {
        return RecoveryOutcome {
            recoverable: Some(false),
            reason: Some("opencode serve not available".into()),
        };
    };

    let client = OpenCodeClient::new(serve.base_url());
    let directory = std::path::Path::new(&worktree_path);

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
    let reattach = client.reattach_events(directory, &session_id, event_tx);

    let mut reattach = Box::pin(reattach);

    loop {
        tokio::select! {
            msg = event_rx.recv() => {
                match msg {
                    Some(live_msg) => {
                        if send_live_message(sender, &live_msg).await.is_err() {
                            return RecoveryOutcome {
                                recoverable: None,
                                reason: None,
                            };
                        }
                    }
                    None => break,
                }
            }
            result = &mut reattach => {
                if let Err(err) = result {
                    let err_msg = err.to_string();
                    if err_msg.contains("not found") {
                        let reason = "server restarted during run";
                        let _ = run_svc.mark_interrupted(run_id, reason).await;
                        return RecoveryOutcome {
                            recoverable: Some(false),
                            reason: Some(format!("interrupted: {reason}")),
                        };
                    }
                    return RecoveryOutcome {
                        recoverable: Some(false),
                        reason: Some(err_msg),
                    };
                }
                break;
            }
        }
    }

    RecoveryOutcome {
        recoverable: Some(false),
        reason: None,
    }
}

fn is_terminal_run_status(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "blocked" | "cancelled")
}

fn is_active_run_status(status: RunStatus) -> bool {
    matches!(status, RunStatus::Queued | RunStatus::Running)
}

fn read_terminal_log_artifact(state: &AppState, run_id: Uuid) -> Option<Vec<u8>> {
    let paths = RunArtifactPaths::new(&state.config.storage.artifacts_dir, &run_id.to_string());
    if !paths.terminal_log.is_file() {
        return None;
    }
    std::fs::read(&paths.terminal_log)
        .ok()
        .filter(|bytes| !bytes.is_empty())
}

async fn send_live_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &LiveMessage,
) -> Result<(), ()> {
    sender
        .send(Message::Text(
            serde_json::to_string(&msg.to_ws_json())
                .unwrap_or_default()
                .into(),
        ))
        .await
        .map_err(|_| ())
}
