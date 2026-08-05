use crate::api::ws::auth::auth_user_from_cookie;
use crate::domain::run::{run_status_to_str, AgentRun, RunStatus};
use crate::events::mark_run_interrupted;
use crate::services::artifact_service::{ArtifactService, RunArtifactPaths};
use crate::services::run_orchestrator::RunOrchestrator;
use crate::services::run_service::{RunError, RunService};
use crate::sessions::opencode_client::OpenCodeClient;
use crate::sessions::{run_registry::RunStreamRegistry, LiveMessage};
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
use std::time::Duration;
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

async fn mark_recovery_interrupted(
    state: &AppState,
    run_id: Uuid,
    reason: &str,
) -> RecoveryOutcome {
    match mark_run_interrupted(state, run_id, reason).await {
        Ok(interrupted) => {
            if let Some(pool) = state.db.as_ref() {
                RunOrchestrator::new(pool, &state.config.workflow)
                    .handle_terminal_run(&interrupted)
                    .await;
            }
            RecoveryOutcome {
                recoverable: Some(false),
                reason: Some(format!("interrupted: {reason}")),
            }
        }
        Err(RunError::Validation(message)) => {
            tracing::debug!(%run_id, %message, "run interruption lost a terminal transition race");
            RecoveryOutcome {
                recoverable: None,
                reason: None,
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, %run_id, "failed to mark recovered run interrupted");
            RecoveryOutcome {
                recoverable: None,
                reason: None,
            }
        }
    }
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
    let is_structured_console = connector
        .as_deref()
        .is_some_and(|connector| matches!(connector, "claude-code" | "codex" | "kilo-code" | "cursor"));

    let stream_handle = if let Some(handle) = state.run_streams.get(run_id) {
        Some(handle)
    } else if is_active_run_status(run.status) {
        wait_for_run_stream(&state, run_id, Duration::from_secs(5)).await
    } else {
        None
    };

    let recovery = if let Some(handle) = stream_handle {
        replay_and_subscribe(&state, run_id, &mut sender, &handle).await;
        None
    } else if is_opencode {
        Some(
            handle_opencode_recovery(&state, &mut sender, run_id, &run).await,
        )
    } else if is_structured_console {
        Some(
            handle_structured_console_recovery(&state, &mut sender, run_id, &run).await,
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

async fn wait_for_run_stream(
    state: &AppState,
    run_id: Uuid,
    max_wait: Duration,
) -> Option<Arc<crate::sessions::run_registry::RunStreamHandle>> {
    // The stream handle is registered before the run is marked running, so a
    // client attaching to an already-active run finds it on the first poll in
    // the common case. This loop is a safety net for the brief dequeue window.
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        if let Some(handle) = state.run_streams.get(run_id) {
            return Some(handle);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        // Bail early once the run transitions to a terminal state so we fall
        // through to artifact replay instead of waiting out the full deadline
        // for a stream that will never appear.
        if let Some(pool) = state.db.as_ref() {
            if let Ok(run) = RunService::new(pool).get(run_id).await {
                if !is_active_run_status(run.status) {
                    return None;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn replay_and_subscribe(
    state: &AppState,
    run_id: Uuid,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    handle: &Arc<crate::sessions::run_registry::RunStreamHandle>,
) {
    if let Some(snapshot) = handle.snapshot() {
        if send_live_message(sender, &LiveMessage::Snapshot { snapshot })
            .await
            .is_err()
        {
            return;
        }
    } else {
        for msg in handle.buffered_tail() {
            if send_live_message(sender, &msg).await.is_err() {
                return;
            }
        }
    }

    let mut rx = handle.subscribe();
    loop {
        let Some(recv) = recv_replay_or_removed(&state.run_streams, run_id, &mut rx).await else {
            break;
        };
        match classify_replay_recv(recv) {
            ReplayRecvAction::Send(msg) => {
                if send_live_message(sender, &msg).await.is_err() {
                    break;
                }
            }
            // Lagged or closed: bail so the handler emits an End (recoverable
            // for active runs) and the client reconnects to re-replay from the
            // snapshot/buffer. Silently continuing would drop missed frames.
            ReplayRecvAction::Break => break,
        }
    }
}

async fn recv_replay_or_removed(
    registry: &RunStreamRegistry,
    run_id: Uuid,
    rx: &mut tokio::sync::broadcast::Receiver<LiveMessage>,
) -> Option<Result<LiveMessage, tokio::sync::broadcast::error::RecvError>> {
    loop {
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(recv) => return Some(recv),
            Err(_) if registry.get(run_id).is_none() => {
                // The handler keeps the broadcast sender alive through its
                // stream handle. Once the registry drops ownership, exit only
                // after the channel backlog has drained so terminal clients do
                // not miss frames queued immediately before completion.
                return None;
            }
            Err(_) => {}
        }
    }
}

/// Decision for one iteration of the replay loop. Extracted so the
/// Lagged -> reconnect reconciliation is independently testable.
enum ReplayRecvAction {
    Send(LiveMessage),
    Break,
}

fn classify_replay_recv(
    recv: Result<LiveMessage, tokio::sync::broadcast::error::RecvError>,
) -> ReplayRecvAction {
    match recv {
        Ok(msg) => ReplayRecvAction::Send(msg),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ReplayRecvAction::Break,
        Err(tokio::sync::broadcast::error::RecvError::Closed) => ReplayRecvAction::Break,
    }
}

async fn handle_opencode_recovery(
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    run_id: Uuid,
    run: &AgentRun,
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
                        return mark_recovery_interrupted(state, run_id, reason).await;
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

/// Replay captured artifacts for claude-code / codex / kilo-code / cursor runs after server restart.
///
/// These connectors run as fresh subprocesses per run. After a server restart the
/// process is gone, so we cannot reattach to a live stream. Instead we replay
/// persisted console events (preferred) or legacy terminal log from disk.
///
/// If the run is still marked active after waiting for a stream handle, the
/// subprocess is gone (e.g. server restarted) — mark it interrupted.
async fn handle_structured_console_recovery(
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    run_id: Uuid,
    run: &AgentRun,
) -> RecoveryOutcome {
    // Replay structured console events from disk (preferred) or legacy terminal log.
    let paths = RunArtifactPaths::new(&state.config.storage.artifacts_dir, &run_id.to_string());
    let console_events = ArtifactService::read_console_events(&paths);
    if !console_events.is_empty() {
        for event in console_events {
            let msg = LiveMessage::Event { event };
            if send_live_message(sender, &msg).await.is_err() {
                return RecoveryOutcome {
                    recoverable: None,
                    reason: None,
                };
            }
        }
    } else if let Some(log_bytes) = read_terminal_log_artifact(state, run_id) {
        let msg = LiveMessage::Frame {
            seq: 0,
            data: log_bytes,
        };
        if send_live_message(sender, &msg).await.is_err() {
            return RecoveryOutcome {
                recoverable: None,
                reason: None,
            };
        }
    }

    // Only running runs without a stream handle after waiting indicate a dead process.
    if run.status == RunStatus::Running {
        let reason = "server restarted during run";
        return mark_recovery_interrupted(state, run_id, reason).await;
    }

    RecoveryOutcome {
        recoverable: Some(is_active_run_status(run.status)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_lagged_breaks_for_reconnect() {
        assert!(matches!(
            classify_replay_recv(Err(tokio::sync::broadcast::error::RecvError::Lagged(3))),
            ReplayRecvAction::Break
        ));
    }

    #[test]
    fn classify_closed_breaks() {
        assert!(matches!(
            classify_replay_recv(Err(tokio::sync::broadcast::error::RecvError::Closed)),
            ReplayRecvAction::Break
        ));
    }

    #[test]
    fn classify_ok_sends() {
        let msg = LiveMessage::Frame { seq: 0, data: vec![] };
        assert!(matches!(
            classify_replay_recv(Ok(msg)),
            ReplayRecvAction::Send(_)
        ));
    }

    /// Overflowing a run stream's broadcast receiver forces Lagged — the
    /// condition the replay loop must turn into a Break so the client
    /// reconnects and re-replays instead of silently dropping frames.
    #[tokio::test]
    async fn run_stream_receiver_lags_on_overflow() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        for i in 0..3000 {
            handle.publish_frame(i, b"x".to_vec());
        }

        let recv = rx.recv().await;
        assert!(
            matches!(recv, Err(tokio::sync::broadcast::error::RecvError::Lagged(_))),
            "expected Lagged after overflow, got {recv:?}"
        );
        assert!(matches!(classify_replay_recv(recv), ReplayRecvAction::Break));
    }

    #[tokio::test]
    async fn removed_stream_drains_queued_messages_before_end() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        for seq in 0..3 {
            handle.publish_frame(seq, vec![seq as u8]);
        }
        registry.remove(run_id);

        let mut received = Vec::new();
        while let Some(recv) = recv_replay_or_removed(&registry, run_id, &mut rx).await {
            match recv.expect("queued message") {
                LiveMessage::Frame { seq, .. } => received.push(seq),
                other => panic!("expected frame, got {other:?}"),
            }
        }

        assert_eq!(received, vec![0, 1, 2]);
    }
}
