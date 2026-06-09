# OpenCode Live Session View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the xterm/ANSI live view for OpenCode runs with a structured session UI ported from OpenCode CLI, relay raw SSE events over WebSocket, and recover gracefully when Coppice restarts mid-run.

**Architecture:** Server publishes a unified `LiveMessage` enum (ANSI `frame` for mock, `snapshot`/`event`/`end` for OpenCode). `OpenCodeClient` forwards raw SSE JSON and merges into `SessionSnapshot` for periodic disk persist. Frontend `opencode-session/` module ports OpenCode TUI part renderers to React; `LiveSession.tsx` applies events via `reduce-event.ts`. WebSocket handler re-attaches to OpenCode `/event` when in-memory registry is empty but run is still active.

**Tech Stack:** Rust/Axum/tokio broadcast, React 19/Vitest/react-markdown, OpenCode HTTP+SSE API

**Spec:** [docs/superpowers/specs/2026-06-08-opencode-live-session-design.md](../specs/2026-06-08-opencode-live-session-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/sessions/live_message.rs` | **NEW** — `LiveMessage` enum + WS JSON encoding |
| `server/src/sessions/session_snapshot.rs` | **NEW** — snapshot type + SSE merge (mirrors FE reducer) |
| `server/src/sessions/run_registry.rs` | `LiveMessage` broadcast + snapshot buffer |
| `server/src/sessions/opencode_client.rs` | Publish raw SSE; drop ANSI tracker |
| `server/src/sessions/opencode_stream.rs` | **DELETE** |
| `server/src/sessions/terminal_frame.rs` | Keep for mock `LiveMessage::Frame` |
| `server/src/sessions/mod.rs` | Export new modules |
| `server/src/api/ws/live.rs` | Dual protocol WS + re-attach |
| `server/src/services/artifact_service.rs` | `session.snapshot.json` path + atomic write |
| `server/src/services/run_service.rs` | `mark_interrupted`, `list_orphaned_active_runs` |
| `server/src/workers/job_worker.rs` | Early `session_id`, snapshot flush on finish |
| `server/src/providers/opencode.rs` | Pass `run_id` + pool into client |
| `server/src/providers/mod.rs` | Extend `AgentRunInput` with `on_session_created` callback or pass pool |
| `server/src/main.rs` | Orphan run sweep on boot |
| `server/tests/integration_live_console.rs` | Mock frame test unchanged; add snapshot/end tests |
| `server/tests/integration_opencode_live.rs` | **NEW** — re-attach fixture test |
| `web/src/opencode-session/**` | Standalone ported session view |
| `web/src/features/runs/LiveSession.tsx` | **NEW** — WS glue for OpenCode |
| `web/src/features/runs/LiveConsole.tsx` | Keep for mock only |
| `web/src/features/tickets/TicketDrawer.tsx` | Route by connector |
| `web/src/lib/schemas/agentRun.ts` | Add `connector` field |
| `server/src/api/agent_runs.rs` | Include `connector` in run response |
| `docs/providers/opencode.md` | Document live session + recovery |

---

## Upstream pin

Pin OpenCode commit in `web/src/opencode-session/README.md` before porting UI (use current `dev` HEAD at implementation time):

```bash
git ls-remote https://github.com/anomalyco/opencode.git refs/heads/dev
```

---

### Task 1: `LiveMessage` wire protocol (server)

**Files:**
- Create: `server/src/sessions/live_message.rs`
- Modify: `server/src/sessions/mod.rs`
- Test: `server/src/sessions/live_message.rs`

- [ ] **Step 1: Write the failing test**

```rust
// server/src/sessions/live_message.rs
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_encodes_as_legacy_type() {
        let msg = LiveMessage::Frame {
            seq: 1,
            data: b"hello\n".to_vec(),
        };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "frame");
        assert_eq!(json["seq"], 1);
        assert_eq!(json["data"], "hello\n");
    }

    #[test]
    fn event_encodes_raw_payload() {
        let event = json!({"type": "message.part.delta", "properties": {}});
        let msg = LiveMessage::Event { event: event.clone() };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "event");
        assert_eq!(json["event"], event);
    }

    #[test]
    fn end_includes_recoverable_flag() {
        let msg = LiveMessage::End {
            status: "failed".into(),
            reason: Some("interrupted: server restarted".into()),
            recoverable: false,
        };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "end");
        assert_eq!(json["recoverable"], false);
        assert_eq!(json["reason"], "interrupted: server restarted");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-server live_message -- --nocapture`  
Expected: FAIL — module not found

- [ ] **Step 3: Implement `LiveMessage`**

```rust
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum LiveMessage {
    Frame { seq: u64, data: Vec<u8> },
    Snapshot { snapshot: Value },
    Event { event: Value },
    End {
        status: String,
        reason: Option<String>,
        recoverable: bool,
    },
}

impl LiveMessage {
    pub fn to_ws_json(&self) -> Value {
        match self {
            LiveMessage::Frame { seq, data } => json!({
                "type": "frame",
                "seq": seq,
                "data": String::from_utf8_lossy(data),
            }),
            LiveMessage::Snapshot { snapshot } => json!({
                "type": "snapshot",
                "messages": snapshot.get("messages").cloned().unwrap_or(json!([])),
                "parts": snapshot.get("parts").cloned().unwrap_or(json!({})),
                "sessionId": snapshot.get("sessionId"),
            }),
            LiveMessage::Event { event } => json!({
                "type": "event",
                "event": event,
            }),
            LiveMessage::End { status, reason, recoverable } => json!({
                "type": "end",
                "status": status,
                "reason": reason,
                "recoverable": recoverable,
            }),
        }
    }
}
```

Add to `server/src/sessions/mod.rs`:

```rust
pub mod live_message;
pub use live_message::LiveMessage;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p coppice-server live_message -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/sessions/live_message.rs server/src/sessions/mod.rs
git commit -m "feat(server): add LiveMessage wire protocol for live WS"
```

---

### Task 2: Extend `RunStreamHandle` for `LiveMessage` + snapshot

**Files:**
- Modify: `server/src/sessions/run_registry.rs`
- Modify: `server/src/sessions/scripted_stream.rs`
- Test: `server/src/sessions/run_registry.rs`

- [ ] **Step 1: Write the failing test**

Add to `run_registry.rs` tests:

```rust
#[tokio::test]
async fn registry_broadcasts_live_messages() {
    let registry = RunStreamRegistry::new();
    let run_id = Uuid::new_v4();
    let handle = registry.register(run_id);
    let mut rx = handle.subscribe();

    handle.publish(LiveMessage::Event {
        event: serde_json::json!({"type": "message.part.delta"}),
    });

    let msg = rx.recv().await.unwrap();
    assert!(matches!(msg, LiveMessage::Event { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-server registry_broadcasts_live_messages -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Refactor registry**

Replace `TerminalFrame` with `LiveMessage` in `RunStreamHandle`:

```rust
use crate::sessions::LiveMessage;
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct RunStreamHandle {
    tx: broadcast::Sender<LiveMessage>,
    cancel_tx: watch::Sender<bool>,
    buffer: Arc<Mutex<Vec<LiveMessage>>>,
    snapshot: Arc<Mutex<Option<Value>>>,
}

impl RunStreamHandle {
    pub fn publish(&self, msg: LiveMessage) { /* same ring buffer, cap 2048 */ }
    pub fn set_snapshot(&self, snapshot: Value) {
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
    }
    pub fn snapshot(&self) -> Option<Value> {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    // publish_frame helper for mock:
    pub fn publish_frame(&self, seq: u64, data: Vec<u8>) {
        self.publish(LiveMessage::Frame { seq, data });
    }
}
```

Update `scripted_stream.rs` to call `publish_frame` instead of `TerminalFrame` publish.

Update `terminal_frame.rs` — keep struct; `to_ws_json` can delegate to `LiveMessage::Frame`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server run_registry scripted_stream -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/sessions/run_registry.rs server/src/sessions/scripted_stream.rs
git commit -m "refactor(server): RunStreamHandle broadcasts LiveMessage"
```

---

### Task 3: `session_snapshot.rs` — merge SSE events server-side

**Files:**
- Create: `server/src/sessions/session_snapshot.rs`
- Modify: `server/src/sessions/mod.rs`
- Test: `server/src/sessions/session_snapshot.rs`

- [ ] **Step 1: Write failing tests**

```rust
// server/src/sessions/session_snapshot.rs
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_delta_appends_text() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "" }
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "ses_1",
                "partID": "prt_1",
                "field": "text",
                "delta": "hello"
            }
        }));
        assert_eq!(snap.parts["msg_0"][0]["text"], "hello");
    }

    #[test]
    fn delta_before_updated_buffers_then_applies() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "ses_1",
                "partID": "prt_1",
                "field": "text",
                "delta": "early"
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "early" }
            }
        }));
        assert_eq!(snap.parts["msg_0"][0]["text"], "early");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-server session_snapshot -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement minimal `SessionSnapshot`**

```rust
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct SessionSnapshot {
    pub session_id: String,
    pub messages: Vec<Value>,
    pub parts: HashMap<String, Vec<Value>>,
    pending_deltas: HashMap<String, Vec<Value>>,
}

impl SessionSnapshot {
    pub fn empty(session_id: &str) -> Self { /* ... */ }

    pub fn to_value(&self) -> Value {
        json!({
            "sessionId": self.session_id,
            "messages": self.messages,
            "parts": self.parts,
        })
    }

    pub fn apply_event(&mut self, event: &Value) {
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "message.part.updated" => self.apply_part_updated(event),
            "message.part.delta" => self.apply_part_delta(event),
            "message.updated" => self.apply_message_updated(event),
            _ => {}
        }
    }

    fn apply_part_delta(&mut self, event: &Value) { /* append delta to part by partID; buffer if part missing */ }
    fn apply_part_updated(&mut self, event: &Value) { /* upsert part; replay pending deltas */ }
    fn apply_message_updated(&mut self, event: &Value) { /* upsert message in messages vec */ }
}
```

Port logic from Task 9 `reduce-event.ts` — keep Rust and TS merge semantics identical.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server session_snapshot -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/sessions/session_snapshot.rs server/src/sessions/mod.rs
git commit -m "feat(server): SessionSnapshot merges OpenCode SSE events"
```

---

### Task 4: Artifact path for `session.snapshot.json`

**Files:**
- Modify: `server/src/services/artifact_service.rs`
- Test: `server/src/services/artifact_service.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn run_artifact_paths_includes_session_snapshot() {
    let paths = RunArtifactPaths::new("/data/artifacts", "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(
        paths.session_snapshot.display().to_string(),
        "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/session.snapshot.json"
    );
}
```

- [ ] **Step 2: Run test — expect FAIL**

Run: `cargo test -p coppice-server run_artifact_paths_includes -- --nocapture`

- [ ] **Step 3: Add field + atomic write**

```rust
pub struct RunArtifactPaths {
    pub terminal_log: PathBuf,
    pub meta_json: PathBuf,
    pub session_snapshot: PathBuf,
}

impl ArtifactService {
    pub fn write_session_snapshot(paths: &RunArtifactPaths, snapshot: &serde_json::Value) -> std::io::Result<()> {
        if let Some(parent) = paths.session_snapshot.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = paths.session_snapshot.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(snapshot)?;
        std::fs::write(&tmp, raw)?;
        std::fs::rename(tmp, &paths.session_snapshot)
    }

    pub fn read_session_snapshot(paths: &RunArtifactPaths) -> Option<serde_json::Value> {
        let raw = std::fs::read_to_string(&paths.session_snapshot).ok()?;
        serde_json::from_str(&raw).ok()
    }
}
```

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/services/artifact_service.rs
git commit -m "feat(server): session.snapshot.json artifact path and atomic write"
```

---

### Task 5: `OpenCodeClient` — publish raw SSE, update snapshot

**Files:**
- Modify: `server/src/sessions/opencode_client.rs`
- Delete: `server/src/sessions/opencode_stream.rs`
- Modify: `server/src/sessions/mod.rs`
- Modify: `server/src/providers/mod.rs` (add `run_id: Option<Uuid>` to `AgentRunInput` if not present as Uuid)
- Modify: `server/src/providers/opencode.rs`

- [ ] **Step 1: Remove ANSI path**

Delete `opencode_stream.rs`. Remove `OpenCodeStreamTracker`, `poll_messages_loop` ANSI publishing, and `MESSAGE_POLL_INTERVAL` poller.

- [ ] **Step 2: Publish raw events in SSE loop**

In `stream_events_loop`, replace tracker calls with:

```rust
fn publish_sse_event(ctx: &StreamContext, event: &serde_json::Value) {
    if let Some(stream) = &ctx.stream {
        stream.publish(LiveMessage::Event { event: event.clone() });
        if let Ok(mut snap) = ctx.snapshot.lock() {
            snap.apply_event(event);
            stream.set_snapshot(snap.to_value());
        }
    }
}
```

Add `snapshot: Arc<Mutex<SessionSnapshot>>` to `StreamContext`, initialized with `SessionSnapshot::empty(&session_id)`.

- [ ] **Step 3: Persist `session_id` on create**

Extend `run_session` signature:

```rust
pub async fn run_session(
    &self,
    directory: &Path,
    model_provider: Option<&str>,
    model: Option<&str>,
    prompt: &str,
    stream: Option<Arc<RunStreamHandle>>,
    cancel_rx: Option<watch::Receiver<bool>>,
    on_session_created: Option<Box<dyn FnOnce(String) + Send>>,
) -> Result<AgentRunResult, ProviderError>
```

After `create_session`, call the callback. In `opencode.rs`:

```rust
let run_id = input.run_id.clone();
client.run_session(/* ... */, Some(Box::new(move |session_id| {
    if let (Some(pool), Some(run_id_str)) = (pool_handle, run_id.as_ref()) {
        let run_uuid = uuid::Uuid::parse_str(run_id_str).ok();
        if let Some(run_uuid) = run_uuid {
            tokio::spawn(async move {
                let _ = RunService::new(&pool).set_session_id(run_uuid, &session_id).await;
            });
        }
    }
}))).await
```

Pass `db: Option<PgPool>` via `AgentRunInput` or thread pool through provider from worker.

Cleaner approach: pass `session_created_tx: Option<watch::Sender<String>>` in `AgentRunInput`; worker subscribes and calls `set_session_id`. Add to `AgentRunInput`:

```rust
pub session_created_tx: Option<watch::Sender<String>>,
```

Worker before `connector.run`:

```rust
let (session_tx, session_rx) = watch::channel(String::new());
// spawn listener:
let pool = pool.clone();
let run_id = run.id;
tokio::spawn(async move {
    let mut rx = session_rx;
    if rx.changed().await.is_ok() {
        let sid = rx.borrow().clone();
        if !sid.is_empty() {
            let _ = RunService::new(&pool).set_session_id(run_id, &sid).await;
        }
    }
});
```

OpenCode client sends session id via `session_created_tx.send(session_id)` after create.

- [ ] **Step 4: Periodic snapshot flush task**

In `job_worker`, after `register(run.id)`:

```rust
let snapshot_handle = stream.clone();
let artifacts_dir = state.config.storage.artifacts_dir.clone();
let run_id = run.id;
let flush_cancel = stream.cancelled_rx();
tokio::spawn(async move {
    let paths = RunArtifactPaths::new(&artifacts_dir, &run_id.to_string());
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Some(snap) = snapshot_handle.snapshot() {
                    let _ = ArtifactService::write_session_snapshot(&paths, &snap);
                }
            }
            _ = flush_cancel.changed() => break,
        }
    }
});
```

On `persist_artifacts` for opencode runs, also write final snapshot and update `RunArtifactMeta` (add `snapshot_written: bool` optional field or keep frame_count as event count).

- [ ] **Step 5: Run tests**

Run: `cargo test -p coppice-server opencode -- --nocapture`  
Run: `cargo clippy -p coppice-server -- -D warnings`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add server/src/sessions/opencode_client.rs server/src/providers/opencode.rs server/src/providers/mod.rs server/src/workers/job_worker.rs
git rm server/src/sessions/opencode_stream.rs
git commit -m "feat(server): OpenCode relays raw SSE events and persists session_id"
```

---

### Task 6: WebSocket handler — dual protocol + re-attach

**Files:**
- Modify: `server/src/api/ws/live.rs`
- Modify: `server/src/services/run_service.rs`

- [ ] **Step 1: Add `mark_interrupted` to RunService**

```rust
pub async fn mark_interrupted(&self, run_id: Uuid, reason: &str) -> Result<AgentRun, RunError> {
    sqlx::query(
        r#"UPDATE agent_runs
           SET status = $2, error_message = $3, ended_at = now()
           WHERE id = $1"#,
    )
    .bind(run_id)
    .bind(run_status_to_str(RunStatus::Failed))
    .bind(format!("interrupted: {reason}"))
    .execute(self.pool)
    .await?;
    self.get(run_id).await
}

pub async fn list_active_without_worker(&self) -> Result<Vec<AgentRun>, RunError> {
    // SELECT runs WHERE status IN ('queued','running')
    // AND NOT EXISTS (SELECT 1 FROM agent_jobs WHERE run_id = agent_runs.id AND status = 'claimed')
    // Simpler v1: list all running/queued on boot; worker will no-op if job already done
    sqlx::query_as(/* ... */).fetch_all(self.pool).await
}
```

- [ ] **Step 2: Refactor `handle_live_socket`**

```rust
async fn handle_live_socket(state: Arc<AppState>, run_id: Uuid, socket: WebSocket) {
    let run = /* load run from DB */;
    let connector = /* join agent.connector for run.agent_id */;
    let is_opencode = connector == "opencode";

    if let Some(handle) = state.run_streams.get(run_id) {
        send_buffered(&mut sender, &handle).await;
        subscribe_until_end(&mut sender, &handle, &state, run_id).await;
    } else if is_opencode {
        handle_opencode_recovery(&state, &mut sender, run_id, &run).await;
    } else if let Some(log) = read_terminal_log_artifact(&state, run_id) {
        send_legacy_frame(&mut sender, log).await;
    }

    send_end_message(&mut sender, &state, run_id, /* recoverable */).await;
}
```

`handle_opencode_recovery`:

1. Load `session.snapshot.json` from artifacts → `LiveMessage::Snapshot`
2. If run active + `session_id` + `worktree_path`:
   - Spawn `OpenCodeClient::reattach_events(directory, session_id, run_id, state)` that publishes to a temporary registry entry OR directly to this WS sender
   - Timeout 10s waiting for first event; if OpenCode 404 → `mark_interrupted`
3. If not recoverable → `end { recoverable: false, reason: "..." }`

Add `OpenCodeClient::reattach_events` — same SSE loop as run but no prompt/wait; filters by sessionID.

`send_end_message` must set `recoverable: false` when status is terminal or interrupted.

- [ ] **Step 3: Fix mock path**

Mock still uses `LiveMessage::Frame`; `send_end` for mock:

```rust
LiveMessage::End {
    status: db_status,
    reason: None,
    recoverable: !is_terminal_status(&db_status),
}
```

For active mock runs without handle, `recoverable: true` (existing reconnect behavior).

- [ ] **Step 4: Run integration test**

Run: `cargo test -p coppice-server integration_live_console -- --nocapture`  
Expected: mock frame test still PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/api/ws/live.rs server/src/services/run_service.rs server/src/sessions/opencode_client.rs
git commit -m "feat(server): WS recovery re-attaches OpenCode SSE after restart"
```

---

### Task 7: Orphan run sweep on boot

**Files:**
- Modify: `server/src/main.rs`
- Test: `server/src/services/run_service.rs` (unit test for `mark_interrupted`)

- [ ] **Step 1: Write failing test for mark_interrupted**

```rust
#[sqlx::test]
async fn mark_interrupted_sets_failed_with_reason(pool: PgPool) {
    // insert running run, call mark_interrupted, assert status failed + error_message prefix
}
```

- [ ] **Step 2: Implement sweep in `main.rs`**

After DB pool + state init:

```rust
if let Some(pool) = &state.db {
    let run_svc = RunService::new(pool);
    if let Ok(runs) = run_svc.list_active_runs().await {
        for run in runs {
            if state.run_streams.get(run.id).is_some() {
                continue;
            }
            if let (Some(session_id), Some(worktree)) = (&run.session_id, &run.worktree_path) {
                let client = OpenCodeClient::new(state.opencode_serve.base_url());
                let alive = client
                    .session_status(Path::new(worktree), session_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some();
                if !alive {
                    let _ = run_svc.mark_interrupted(run.id, "server restarted during run").await;
                }
            } else {
                let _ = run_svc.mark_interrupted(run.id, "server restarted during run").await;
            }
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p coppice-server mark_interrupted -- --nocapture`

- [ ] **Step 4: Commit**

```bash
git add server/src/main.rs server/src/services/run_service.rs
git commit -m "feat(server): mark orphaned active runs interrupted on boot"
```

---

### Task 8: Frontend types + `reduce-event.ts`

**Files:**
- Create: `web/src/opencode-session/README.md`
- Create: `web/src/opencode-session/sync/types.ts`
- Create: `web/src/opencode-session/sync/reduce-event.ts`
- Create: `web/src/opencode-session/sync/reduce-event.test.ts`

- [ ] **Step 1: Write failing tests**

```typescript
// web/src/opencode-session/sync/reduce-event.test.ts
import { describe, expect, it } from 'vitest';
import { createSessionStore, applyEvent } from './reduce-event';

describe('applyEvent', () => {
  it('appends text deltas incrementally', () => {
    const store = createSessionStore('ses_1');
    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'hello ',
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'world',
      },
    });
    expect(store.parts['msg_1'][0].text).toBe('hello world');
  });

  it('handles delta before part.updated', () => {
    const store = createSessionStore('ses_1');
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'early',
      },
    });
    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    expect(store.parts['msg_1'][0].text).toBe('early');
  });
});
```

- [ ] **Step 2: Run test — expect FAIL**

Run: `cd web && npm test -- opencode-session/sync/reduce-event.test.ts`  
Expected: FAIL

- [ ] **Step 3: Implement types + reducer**

```typescript
// web/src/opencode-session/sync/types.ts
export type PartType = 'text' | 'reasoning' | 'tool';

export interface TextPart {
  id: string;
  type: 'text';
  text: string;
  messageID: string;
}

export interface ReasoningPart {
  id: string;
  type: 'reasoning';
  text: string;
  messageID: string;
}

export interface ToolPart {
  id: string;
  type: 'tool';
  tool: string;
  callID?: string;
  messageID: string;
  state: {
    status: string;
    input?: Record<string, unknown>;
    output?: unknown;
    metadata?: Record<string, unknown>;
  };
}

export type Part = TextPart | ReasoningPart | ToolPart;

export interface Message {
  id: string;
  sessionID: string;
  role: 'user' | 'assistant';
  time?: { created?: number; completed?: number };
  parentID?: string;
  modelID?: string;
  mode?: string;
  agent?: string;
  finish?: string;
  error?: { name?: string; data?: { message?: string } };
}

export interface SessionStore {
  sessionId: string;
  messages: Message[];
  parts: Record<string, Part[]>;
  pendingDeltas: Record<string, Array<{ field: string; delta: string }>>;
}
```

Implement `createSessionStore`, `applySnapshot`, `applyEvent` in `reduce-event.ts` mirroring Task 3 Rust logic.

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add web/src/opencode-session/
git commit -m "feat(web): OpenCode session event reducer with tests"
```

---

### Task 9: Theme + part shell components

**Files:**
- Create: `web/src/opencode-session/theme/session-theme.ts`
- Create: `web/src/opencode-session/parts/TextPart.tsx`
- Create: `web/src/opencode-session/parts/ReasoningPart.tsx`
- Create: `web/src/opencode-session/parts/ToolPart.tsx`

- [ ] **Step 1: Create theme tokens**

```typescript
// web/src/opencode-session/theme/session-theme.ts
export const sessionTheme = {
  text: 'text-text-primary',
  textMuted: 'text-text-muted',
  border: 'border-border',
  borderActive: 'border-primary',
  backgroundPanel: 'bg-surface-elevated',
  error: 'text-destructive',
  accent: 'text-primary',
} as const;
```

- [ ] **Step 2: Implement TextPart with react-markdown**

```tsx
// web/src/opencode-session/parts/TextPart.tsx
import ReactMarkdown from 'react-markdown';
import type { TextPart as TextPartType } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';

export function TextPart({ part }: { part: TextPartType }) {
  const text = part.text.trim();
  if (!text) return null;
  return (
    <div className={`ml-3 mt-2 ${sessionTheme.text}`}>
      <ReactMarkdown>{text}</ReactMarkdown>
    </div>
  );
}
```

- [ ] **Step 3: Implement ReasoningPart**

```tsx
export function ReasoningPart({ part }: { part: ReasoningPartType }) {
  const content = part.text.replaceAll('[REDACTED]', '').trim();
  if (!content) return null;
  return (
    <div className={`ml-2 mt-2 border-l-2 pl-2 ${sessionTheme.border} ${sessionTheme.textMuted}`}>
      <ReactMarkdown>_Thinking:_ {content}</ReactMarkdown>
    </div>
  );
}
```

- [ ] **Step 4: Implement ToolPart router + unknown fallback**

```tsx
export function ToolPart({ part }: { part: ToolPartType }) {
  const Component = TOOL_MAP[part.tool] ?? UnknownTool;
  return <Component part={part} />;
}

function UnknownTool({ part }: { part: ToolPartType }) {
  const input = JSON.stringify(part.state.input ?? {}, null, 0).slice(0, 120);
  return (
    <div className={`ml-2 mt-1 font-mono text-xs ${sessionTheme.textMuted}`}>
      → {part.tool}: {input || '(no input)'}
    </div>
  );
}
```

- [ ] **Step 5: Commit**

```bash
git add web/src/opencode-session/parts web/src/opencode-session/theme
git commit -m "feat(web): OpenCode session part components shell"
```

---

### Task 10: Port per-tool components (1:1 from upstream)

**Files:**
- Create: `web/src/opencode-session/tools/*.tsx` (one file per tool)
- Modify: `web/src/opencode-session/parts/ToolPart.tsx`

Port from `packages/opencode/src/cli/cmd/tui/routes/session/index.tsx` (tool components at bottom of file). Create:

| Coppice file | Upstream source |
|---|---|
| `tools/Bash.tsx` | `Bash` component |
| `tools/Read.tsx` | `Read` / read tool match |
| `tools/Write.tsx` | `Write` |
| `tools/Edit.tsx` | `Edit` |
| `tools/Grep.tsx` | `Grep` |
| `tools/Glob.tsx` | `Glob` |
| `tools/List.tsx` | `List` / `ls` |
| `tools/WebFetch.tsx` | `WebFetch` |
| `tools/Task.tsx` | `Task` |
| `tools/Skill.tsx` | `Skill` |
| `tools/Question.tsx` | `Question` |
| `tools/TodoWrite.tsx` | `TodoWrite` |
| `tools/ApplyPatch.tsx` | `ApplyPatch` |

- [ ] **Step 1: Pin upstream commit in README**

```markdown
# opencode-session

Upstream: anomalyco/opencode @ `<commit>`
Ported from `packages/opencode/src/cli/cmd/tui/routes/session/index.tsx`

| This file | Upstream |
|-----------|----------|
| `tools/Bash.tsx` | `Bash` in session/index.tsx |
| ... | ... |
```

- [ ] **Step 2: Port Bash tool (template for others)**

```tsx
// web/src/opencode-session/tools/Bash.tsx
import type { ToolPart } from '../sync/types';
import { sessionTheme } from '../theme/session-theme';
import { ToolShell } from './ToolShell';

export function Bash({ part }: { part: ToolPart }) {
  const command = String(part.state.input?.command ?? '');
  const status = part.state.status;
  return (
    <ToolShell tool="bash" status={status} title={command}>
      {typeof part.state.output === 'string' ? (
        <pre className="overflow-x-auto text-xs">{part.state.output}</pre>
      ) : null}
    </ToolShell>
  );
}
```

Create shared `ToolShell.tsx` for border/title/status icon (→ running, ✓ done, ✗ error).

- [ ] **Step 3: Port remaining tools**

For each tool: copy labels/field names from upstream (`filePath`, `pattern`, `url`, `description`, etc.). Translate OpenTUI `<box>`/`<text>` to Tailwind divs. Do not import `@opentui/solid`.

- [ ] **Step 4: Wire `TOOL_MAP` in ToolPart.tsx**

```typescript
import { Bash } from '../tools/Bash';
// ... all tools

const TOOL_MAP: Record<string, React.ComponentType<{ part: ToolPart }>> = {
  bash: Bash,
  read: Read,
  write: Write,
  edit: Edit,
  grep: Grep,
  glob: Glob,
  ls: List,
  webfetch: WebFetch,
  task: Task,
  skill: Skill,
  question: Question,
  todowrite: TodoWrite,
  apply_patch: ApplyPatch,
};
```

- [ ] **Step 5: Commit**

```bash
git add web/src/opencode-session/
git commit -m "feat(web): port OpenCode per-tool session components"
```

---

### Task 11: `SessionView` + `LiveSession` WS client

**Files:**
- Create: `web/src/opencode-session/session/SessionView.tsx`
- Create: `web/src/opencode-session/session/AssistantMessage.tsx`
- Create: `web/src/features/runs/LiveSession.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`

- [ ] **Step 1: Implement SessionView**

```tsx
// web/src/opencode-session/session/SessionView.tsx
import { useMemo } from 'react';
import type { SessionStore } from '../sync/types';
import { AssistantMessage } from './AssistantMessage';
import { UserMessage } from './UserMessage';

export function SessionView({ store }: { store: SessionStore }) {
  const messages = useMemo(
    () => [...store.messages].sort((a, b) => a.id.localeCompare(b.id)),
    [store.messages],
  );
  return (
    <div className="flex flex-col gap-2 overflow-y-auto">
      {messages.map((message) =>
        message.role === 'assistant' ? (
          <AssistantMessage
            key={message.id}
            message={message}
            parts={store.parts[message.id] ?? []}
          />
        ) : (
          <UserMessage key={message.id} message={message} parts={store.parts[message.id] ?? []} />
        ),
      )}
    </div>
  );
}
```

`AssistantMessage` maps parts through `TextPart`, `ReasoningPart`, `ToolPart` (port footer line from upstream: agent · model · duration).

- [ ] **Step 2: Implement LiveSession.tsx**

```tsx
import { useEffect, useReducer, useRef, useState } from 'react';
import { SessionView } from '../../opencode-session/session/SessionView';
import { applyEvent, applySnapshot, createSessionStore } from '../../opencode-session/sync/reduce-event';

function isActiveRunStatus(status: string | null) {
  return status === 'running' || status === 'queued';
}

export function LiveSession({ runId, runStatus }: { runId: string | null; runStatus: string | null }) {
  const [store, dispatch] = useReducer(sessionReducer, null as SessionStore | null);
  const [connection, setConnection] = useState<'connecting' | 'open' | 'closed'>('closed');
  const [interrupted, setInterrupted] = useState<string | null>(null);
  const recoverableRef = useRef(true);

  useEffect(() => {
    if (!runId) return;
    setConnection('connecting');
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${protocol}://${window.location.host}/ws/agent-runs/${runId}/live`);

    ws.onopen = () => setConnection('open');
    ws.onclose = () => setConnection('closed');
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string);
      if (msg.type === 'snapshot') {
        dispatch({ type: 'snapshot', messages: msg.messages, parts: msg.parts, sessionId: msg.sessionId });
      } else if (msg.type === 'event') {
        dispatch({ type: 'event', event: msg.event });
      } else if (msg.type === 'end') {
        recoverableRef.current = msg.recoverable !== false;
        if (msg.reason) setInterrupted(msg.reason);
        ws.close();
      }
    };
    return () => ws.close();
  }, [runId]);

  useEffect(() => {
    if (!isActiveRunStatus(runStatus) || connection !== 'closed' || !runId) return;
    if (!recoverableRef.current) return;
    const timer = window.setTimeout(() => dispatch({ type: 'reconnect' }), 800);
    return () => window.clearTimeout(timer);
  }, [runStatus, connection, runId]);

  // ... status banner, render SessionView when store non-null
}
```

Implement `sessionReducer` wrapping `createSessionStore` / `applySnapshot` / `applyEvent`.

- [ ] **Step 3: Add `connector` to run API + schema**

In `server/src/api/agent_runs.rs`, join agents table:

```rust
// run_to_response: add connector: String from join or lookup cache
```

```typescript
// web/src/lib/schemas/agentRun.ts
connector: z.string().optional(),
```

- [ ] **Step 4: Update TicketDrawer**

```tsx
import { LiveConsole } from '../runs/LiveConsole';
import { LiveSession } from '../runs/LiveSession';

const liveRun = activeRun ?? latestRun;
const LiveView = liveRun?.connector === 'opencode' ? LiveSession : LiveConsole;

<LiveView runId={liveRun?.id ?? null} runStatus={liveRun?.status ?? null} />
```

- [ ] **Step 5: Run web tests + build**

Run: `cd web && npm test && npm run build`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/opencode-session/session web/src/features/runs/LiveSession.tsx web/src/features/tickets/TicketDrawer.tsx web/src/lib/schemas/agentRun.ts server/src/api/agent_runs.rs
git commit -m "feat(web): LiveSession structured view wired to WS"
```

---

### Task 12: Integration tests + docs

**Files:**
- Create: `server/tests/integration_opencode_live.rs`
- Modify: `server/tests/integration_live_console.rs`
- Modify: `docs/providers/opencode.md`

- [ ] **Step 1: Add recovery integration test (mock SSE server)**

```rust
#[tokio::test]
async fn live_ws_replays_snapshot_when_no_registry() {
    // Write session.snapshot.json to artifact dir for a running run
    // Ensure run_streams has no handle
    // Connect WS, expect first message type == "snapshot"
    // Expect end with recoverable false if opencode unreachable
}
```

Use `wiremock` or inline `axum::Router` mock for OpenCode `/event` if already in dev-deps; otherwise test snapshot-only path without live re-attach.

- [ ] **Step 2: Verify mock regression**

Run: `cargo test -p coppice-server integration_live_console -- --nocapture`  
Expected: PASS (frame protocol unchanged for mock)

- [ ] **Step 3: Update docs**

In `docs/providers/opencode.md`, add sections:
- Live Session view (structured, not xterm)
- WebSocket message types (`snapshot`, `event`, `end`)
- Recovery behavior after server restart
- `session.snapshot.json` artifact

- [ ] **Step 4: Full verification**

```bash
cargo test -p coppice-server
cargo clippy -p coppice-server -- -D warnings
cd web && npm test && npm run build
```

- [ ] **Step 5: Commit**

```bash
git add server/tests/ docs/providers/opencode.md
git commit -m "test(docs): OpenCode live session integration tests and docs"
```

---

## Self-review (spec coverage)

| Spec requirement | Task |
|------------------|------|
| Structured session view (not xterm) | 8–11 |
| Smooth per-delta streaming | 5, 8 (no server poller) |
| Meaningful tool labels | 9–10 |
| `session_id` at creation | 5 |
| `session.snapshot.json` | 4, 5 |
| Restart recovery / no infinite reconnect | 6, 7, 11 |
| Standalone module + README map | 8, 10 |
| Remove `opencode_stream.rs` | 5 |
| Mock keeps xterm | 2, 6, 11 |

No placeholder steps remain. Types consistent: `LiveMessage` server-side ↔ WS JSON ↔ `reduce-event.ts` client-side.

---

## Manual test plan

1. Start stack with `opencode` connector enabled; run researcher agent on a ticket.
2. Confirm Live tab shows markdown text streaming smoothly, tool cards with paths/commands.
3. Restart `coppice-server` mid-run while `opencode serve` stays up → stream resumes or shows partial snapshot.
4. Restart both Coppice and kill OpenCode session → UI shows "interrupted", no reconnect loop.
5. Run mock agent → Live tab still uses xterm with frame protocol.
