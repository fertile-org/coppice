use crate::api::ws::auth::auth_user_from_cookie;
use crate::domain::run::run_status_to_str;
use crate::events::bus::AppEvent;
use crate::services::run_service::RunService;
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
use std::time::{Duration, Instant};

/// Cadence at which `/ws/events` sends a WebSocket Ping to detect half-open
/// connections. Mirrors the per-run watchdog heartbeat cadence
/// (`run_watchdog.rs`).
const EVENTS_KEEPALIVE_INTERVAL_SECS: u64 = 30;

/// How many keepalive intervals may pass without a Pong before the socket is
/// considered stale (half-open TCP) and closed. At 30s/interval this is a ~90s
/// grace, well within typical NAT timeouts but fast enough to trigger a client
/// reconnect instead of silently losing events.
const EVENTS_KEEPALIVE_MAX_MISSED: u64 = 3;

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
    let (mut sender, mut receiver) = socket.split();

    // Subscribe BEFORE reading the snapshot so events published between the DB
    // read and the subscription are still delivered. Overlap (a synthetic
    // started followed by the real finished) is harmless: the frontend patches
    // cache then invalidates, idempotently.
    let mut rx = state.event_bus.subscribe();

    // Late-subscriber reconciliation: emit current truth for active runs so a
    // client connecting AFTER agent_run.started fired learns the run is active
    // without depending on catching the (already-missed) broadcast event.
    if emit_active_run_snapshot(&state, &mut sender).await.is_err() {
        return;
    }

    let mut ping_interval = tokio::time::interval(Duration::from_secs(EVENTS_KEEPALIVE_INTERVAL_SECS));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Discard the immediate first tick so the first ping goes out after one
    // full interval, matching the watchdog cadence.
    let _ = ping_interval.tick().await;
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            biased;
            // Drive the read half: process Pongs (keepalive), respond to pings,
            // and tear down on explicit close / peer disconnect.
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = Instant::now();
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = ping_interval.tick() => {
                if keepalive_is_stale(last_pong, Instant::now()) {
                    // No Pong within the grace window: treat as half-open and
                    // close so the client reconnects and re-syncs.
                    break;
                }
                if sender.send(Message::Ping(keepalive_payload().into())).await.is_err() {
                    break;
                }
            }
            recv = rx.recv() => {
                match classify_event_recv(recv) {
                    EventRecvAction::Send(raw) => {
                        if sender.send(Message::Text(raw.into())).await.is_err() {
                            break;
                        }
                    }
                    EventRecvAction::Resync => {
                        if sender.send(Message::Text(resync_message().into())).await.is_err() {
                            break;
                        }
                    }
                    EventRecvAction::Continue => {}
                    EventRecvAction::Close => break,
                }
            }
        }
    }
}

/// Emit a synthetic `agent_run.started` for every currently-active run so a
/// freshly connected client receives current truth. Returns `Err(())` only when
/// the socket is no longer writable.
async fn emit_active_run_snapshot(
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> Result<(), ()> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(());
    };
    let runs = RunService::new(pool).list_active_runs().await.unwrap_or_default();
    for run in runs {
        let event = AppEvent::AgentRunStarted {
            run_id: run.id,
            ticket_id: run.ticket_id,
            agent_id: run.agent_id,
            status: run_status_to_str(run.status).to_string(),
        };
        let Ok(raw) = serde_json::to_string(&event) else {
            continue;
        };
        if sender.send(Message::Text(raw.into())).await.is_err() {
            return Err(());
        }
    }
    Ok(())
}

/// What the event loop should do for a given broadcast recv result. Extracted
/// so the Lagged -> resync mapping is independently testable.
#[derive(Debug)]
enum EventRecvAction {
    /// Send the serialized event payload.
    Send(String),
    /// Receiver lagged: signal the client to reconcile.
    Resync,
    /// Transient: skip this recv without sending.
    Continue,
    /// Channel closed or unrecoverable: stop the loop.
    Close,
}

fn classify_event_recv(
    recv: Result<AppEvent, tokio::sync::broadcast::error::RecvError>,
) -> EventRecvAction {
    match recv {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(raw) => EventRecvAction::Send(raw),
            Err(_) => EventRecvAction::Continue,
        },
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => EventRecvAction::Resync,
        Err(tokio::sync::broadcast::error::RecvError::Closed) => EventRecvAction::Close,
    }
}

/// Transport-level marker (NOT an `AppEvent` variant) sent on a lagged socket
/// to tell the client its view may be stale and it should re-fetch.
fn resync_message() -> &'static str {
    r#"{"type":"resync"}"#
}

fn keepalive_payload() -> Vec<u8> {
    vec![b'k']
}

fn keepalive_is_stale(last_pong: Instant, now: Instant) -> bool {
    let grace = Duration::from_secs(EVENTS_KEEPALIVE_INTERVAL_SECS * EVENTS_KEEPALIVE_MAX_MISSED);
    now.saturating_duration_since(last_pong) > grace
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::bus::{AppEvent, EventBus};
    use uuid::Uuid;

    #[test]
    fn resync_message_is_valid_json_with_resync_type() {
        let raw = resync_message();
        let json: serde_json::Value = serde_json::from_str(raw).expect("valid json");
        assert_eq!(json["type"], "resync");
    }

    #[test]
    fn classify_ok_event_serializes_to_send() {
        let event = AppEvent::AgentRunStarted {
            run_id: Uuid::nil(),
            ticket_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            status: "running".into(),
        };
        match classify_event_recv(Ok(event)) {
            EventRecvAction::Send(raw) => {
                let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
                assert_eq!(json["type"], "agent_run.started");
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn classify_lagged_maps_to_resync() {
        assert!(matches!(
            classify_event_recv(Err(tokio::sync::broadcast::error::RecvError::Lagged(7))),
            EventRecvAction::Resync
        ));
    }

    #[test]
    fn classify_closed_maps_to_close() {
        assert!(matches!(
            classify_event_recv(Err(tokio::sync::broadcast::error::RecvError::Closed)),
            EventRecvAction::Close
        ));
    }

    #[tokio::test]
    async fn lagged_receiver_emits_resync_signal() {
        // The global bus uses a 256-slot broadcast channel. Flooding it while a
        // subscriber exists but is not draining forces a Lagged recv, which the
        // handler must surface as a resync rather than silently dropping.
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        for _ in 0..300 {
            bus.publish(AppEvent::TicketUpdated {
                ticket_id: Uuid::nil(),
                status: "in_progress".into(),
                substatus: None,
                updated_at: "2026-01-01T00:00:00Z".into(),
            });
        }

        let recv = rx.recv().await;
        assert!(
            matches!(recv, Err(tokio::sync::broadcast::error::RecvError::Lagged(_))),
            "expected Lagged after overflow, got {recv:?}"
        );
        assert!(matches!(
            classify_event_recv(recv),
            EventRecvAction::Resync
        ));
    }

    #[test]
    fn keepalive_is_stale_true_beyond_grace() {
        let now = Instant::now();
        let grace = Duration::from_secs(EVENTS_KEEPALIVE_INTERVAL_SECS * EVENTS_KEEPALIVE_MAX_MISSED);
        let last_pong = now - grace - Duration::from_secs(1);
        assert!(keepalive_is_stale(last_pong, now));
    }

    #[test]
    fn keepalive_is_stale_false_within_grace() {
        let now = Instant::now();
        let last_pong = now - Duration::from_secs(5);
        assert!(!keepalive_is_stale(last_pong, now));
    }
}
