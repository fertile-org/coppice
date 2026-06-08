# M04 Live Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver live terminal streaming (xterm.js), WebSocket board events, run-completion toasts, terminal log artifacts, Stop wired to active sessions, MockProvider streaming for CI, and OpenCode provider with auto-started `opencode serve` for manual testing.

**Architecture:** Tiered stream backends (`ScriptedStream` for mock, `OpenCodeStream` for API-key testing) normalize to `TerminalFrame` and fan out via an in-process broadcast registry. `job_worker` creates a per-run stream before calling `AgentProvider::run`, persists `terminal.log` on finish, and emits lifecycle events on `EventBus`. WebSocket handlers authenticate via session cookie (same as HTTP middleware).

**Tech Stack:** Rust/Axum WebSocket/tokio::sync::broadcast, React/xterm.js/@xterm/addon-fit, Vitest, Node smoke E2E

**Spec:** [docs/superpowers/specs/2026-06-08-m04-live-console-design.md](../specs/2026-06-08-m04-live-console-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/004_live_console.sql` | `agent_runs.session_id` column |
| `server/src/sessions/terminal_frame.rs` | `TerminalFrame`, WS JSON encoding |
| `server/src/sessions/run_registry.rs` | Per-run broadcast + cancel handles |
| `server/src/sessions/scripted_stream.rs` | Mock chunked output emitter |
| `server/src/sessions/opencode_serve.rs` | Auto-start/stop `opencode serve` child |
| `server/src/sessions/opencode_events.rs` | Map OpenCode JSON lines → frames + result |
| `server/src/sessions/tmux_stream.rs` | Stub module for future CLI providers |
| `server/src/sessions/mod.rs` | Module root |
| `server/src/events/bus.rs` | `EventBus` broadcast for `/ws/events` |
| `server/src/events/mod.rs` | Module root |
| `server/src/services/artifact_service.rs` | `terminal.log` + `meta.json` paths/writes |
| `server/src/providers/opencode.rs` | `OpenCodeProvider` impl |
| `server/src/providers/mod.rs` | Extend `AgentRunInput` with stream/cancel |
| `server/src/providers/mock.rs` | Emit scripted frames during `run` |
| `server/src/api/ws/live.rs` | `GET /ws/agent-runs/:id/live` |
| `server/src/api/ws/events.rs` | `GET /ws/events` |
| `server/src/api/ws/mod.rs` | WS route wiring |
| `server/src/api/mod.rs` | Mount WS routes (session auth, no CSRF) |
| `server/src/workers/job_worker.rs` | Stream lifecycle, events, artifacts |
| `server/src/services/run_service.rs` | `session_id` column, cancel trigger hook |
| `server/src/domain/run.rs` | `session_id` field on `AgentRun` |
| `server/src/lib.rs` | `sessions`, `events` modules; extend `AppState` |
| `server/src/main.rs` | Start OpenCode serve on boot; graceful shutdown |
| `server/Cargo.toml` | `axum` ws feature, `reqwest`, `tokio-tungstenite` dev-dep |
| `config/src/lib.rs` | `OpenCodeProviderConfig`, nested under `AgentConfig` |
| `config.example.toml` | OpenCode provider section |
| `deploy/config/default.toml` | Stays `mock` only |
| `fixtures/opencode-events/sample.jsonl` | Unit test fixture for event mapping |
| `server/tests/integration_live_console.rs` | WS + artifact integration tests |
| `web/package.json` | `xterm`, `@xterm/addon-fit` |
| `web/src/features/runs/LiveConsole.tsx` | xterm.js + WS client |
| `web/src/features/ws/useEventSocket.ts` | Board invalidation + run toasts |
| `web/src/features/tickets/TicketDrawer.tsx` | Live Console tab |
| `web/src/features/board/TicketCard.tsx` | Live-run badge |
| `web/src/features/board/BoardPage.tsx` | Pass `runningTicketIds` to cards |
| `web/src/components/ToastProvider.tsx` | Persistent error toasts + click handler |
| `web/src/App.tsx` | Mount `useEventSocket` inside auth |
| `e2e/smoke/m04-live-console.mjs` | API-level live stream smoke (WS frames) |
| `Makefile` | `e2e-smoke-m04` target |
| `docs/milestones/M04-live-console.md` | Check off acceptance criteria when done |

---

### Task 1: Migration + `session_id` on domain and API

**Files:**
- Create: `server/migrations/004_live_console.sql`
- Modify: `server/src/domain/run.rs`
- Modify: `server/src/services/run_service.rs`
- Modify: `server/src/api/agent_runs.rs`

- [ ] **Step 1: Write migration**

Create `server/migrations/004_live_console.sql`:

```sql
ALTER TABLE agent_runs
  ADD COLUMN IF NOT EXISTS session_id TEXT NULL;
```

- [ ] **Step 2: Extend `AgentRun` domain type**

In `server/src/domain/run.rs`, add field:

```rust
pub struct AgentRun {
    // ...existing fields...
    pub session_id: Option<String>,
}
```

- [ ] **Step 3: Update `row_to_run` and all SQL RETURNING/SELECT lists**

In `server/src/services/run_service.rs`, include `session_id` in every `agent_runs` query that maps to `AgentRun`. Add helper:

```rust
pub async fn set_session_id(&self, run_id: Uuid, session_id: &str) -> Result<(), RunError> {
    sqlx::query("UPDATE agent_runs SET session_id = $2 WHERE id = $1")
        .bind(run_id)
        .bind(session_id)
        .execute(self.pool)
        .await?;
    Ok(())
}
```

- [ ] **Step 4: Expose `sessionId` in API responses**

In `server/src/api/agent_runs.rs`, add to `RunResponse`:

```rust
session_id: Option<String>,
```

Map from `run.session_id` in `run_to_response`.

- [ ] **Step 5: Run migration locally**

```bash
make migrate
```

Expected: migration applies cleanly.

- [ ] **Step 6: Run tests**

```bash
cargo test -p coppice-server run_status
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add server/migrations/004_live_console.sql server/src/domain/run.rs server/src/services/run_service.rs server/src/api/agent_runs.rs
git commit -m "feat(server): add session_id column for agent runs"
```

---

### Task 2: OpenCode provider config

**Files:**
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`
- Modify: `deploy/config/default.toml`

- [ ] **Step 1: Add config structs**

In `config/src/lib.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub default_provider: String,
    pub worktrees_path: String,
    pub worker_count: u32,
    #[serde(default)]
    pub providers: AgentProvidersConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentProvidersConfig {
    #[serde(default)]
    pub opencode: OpenCodeProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeProviderConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_opencode_command")]
    pub command: String,
    #[serde(default = "default_opencode_host")]
    pub serve_hostname: String,
    #[serde(default = "default_opencode_port")]
    pub serve_port: u16,
    pub model: Option<String>,
    pub variant: Option<String>,
}

fn default_false() -> bool { false }
fn default_opencode_command() -> String { "opencode".into() }
fn default_opencode_host() -> String { "127.0.0.1".into() }
fn default_opencode_port() -> u16 { 4096 }
```

Update `default_values()`:

```rust
agent: AgentConfig {
    default_provider: "mock".into(),
    worktrees_path: "./data/worktrees".into(),
    worker_count: 2,
    providers: AgentProvidersConfig::default(),
},
```

Add env merge for `AGENT_DEFAULT_PROVIDER` (existing) — no new env required for M04.

- [ ] **Step 2: Update example configs**

`config.example.toml`:

```toml
[agent.providers.opencode]
enabled = false
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
# model = "anthropic/claude-sonnet-4-20250514"
```

`deploy/config/default.toml` — omit opencode section (disabled by default).

- [ ] **Step 3: Run config tests**

```bash
cargo test -p coppice-config
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add config/src/lib.rs config.example.toml deploy/config/default.toml
git commit -m "feat(config): add OpenCode provider settings"
```

---

### Task 3: TerminalFrame + ArtifactService (TDD)

**Files:**
- Create: `server/src/sessions/terminal_frame.rs`
- Create: `server/src/sessions/mod.rs`
- Create: `server/src/services/artifact_service.rs`
- Modify: `server/src/services/mod.rs`
- Modify: `server/src/lib.rs`

- [ ] **Step 1: Write failing unit tests for frames**

Create `server/src/sessions/terminal_frame.rs` with `#[cfg(test)]`:

```rust
#[test]
fn ws_message_roundtrip() {
    let frame = TerminalFrame {
        seq: 1,
        data: b"Mock agent starting...\n".to_vec(),
        ts: OffsetDateTime::now_utc(),
    };
    let json = frame.to_ws_json();
    assert_eq!(json["type"], "frame");
    assert_eq!(json["seq"], 1);
    assert!(json["data"].as_str().unwrap().contains("Mock agent"));
}
```

- [ ] **Step 2: Run test — expect fail**

```bash
cargo test -p coppice-server terminal_frame::tests::ws_message_roundtrip -- --nocapture
```

Expected: FAIL (module/type not found)

- [ ] **Step 3: Implement `TerminalFrame`**

```rust
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    pub seq: u64,
    pub data: Vec<u8>,
    pub ts: OffsetDateTime,
}

impl TerminalFrame {
    pub fn to_ws_json(&self) -> serde_json::Value {
        json!({
            "type": "frame",
            "seq": self.seq,
            "data": String::from_utf8_lossy(&self.data),
        })
    }

    pub fn end_message(status: &str) -> serde_json::Value {
        json!({ "type": "end", "status": status })
    }
}
```

- [ ] **Step 4: Write failing artifact path test**

In `server/src/services/artifact_service.rs`:

```rust
#[test]
fn run_artifact_paths() {
    let paths = RunArtifactPaths::new("/data/artifacts", "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(
        paths.terminal_log.display().to_string(),
        "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/terminal.log"
    );
    assert_eq!(
        paths.meta_json.display().to_string(),
        "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/meta.json"
    );
}
```

- [ ] **Step 5: Implement `ArtifactService`**

```rust
use std::path::PathBuf;
use serde::Serialize;
use time::OffsetDateTime;

pub struct RunArtifactPaths {
    pub terminal_log: PathBuf,
    pub meta_json: PathBuf,
}

impl RunArtifactPaths {
    pub fn new(artifacts_dir: &str, run_id: &str) -> Self {
        let base = PathBuf::from(artifacts_dir).join("runs").join(run_id);
        Self {
            terminal_log: base.join("terminal.log"),
            meta_json: base.join("meta.json"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifactMeta {
    pub provider: String,
    pub session_id: Option<String>,
    pub frame_count: u64,
    pub ended_at: String,
}

pub struct ArtifactService;

impl ArtifactService {
    pub fn write_terminal_log(paths: &RunArtifactPaths, content: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = paths.terminal_log.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&paths.terminal_log, content)
    }

    pub fn write_meta(paths: &RunArtifactPaths, meta: &RunArtifactMeta) -> std::io::Result<()> {
        let raw = serde_json::to_vec_pretty(meta)?;
        std::fs::write(&paths.meta_json, raw)
    }
}
```

- [ ] **Step 6: Wire modules**

`server/src/sessions/mod.rs`:

```rust
pub mod terminal_frame;
pub use terminal_frame::TerminalFrame;
```

`server/src/lib.rs`: add `pub mod sessions;`

- [ ] **Step 7: Run tests**

```bash
cargo test -p coppice-server terminal_frame artifact_service
```

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add server/src/sessions server/src/services/artifact_service.rs server/src/services/mod.rs server/src/lib.rs
git commit -m "feat(server): add TerminalFrame and run artifact paths"
```

---

### Task 4: Run stream registry + scripted stream

**Files:**
- Create: `server/src/sessions/run_registry.rs`
- Create: `server/src/sessions/scripted_stream.rs`
- Modify: `server/src/sessions/mod.rs`

- [ ] **Step 1: Write failing registry test**

```rust
#[tokio::test]
async fn registry_broadcasts_frames() {
    let registry = RunStreamRegistry::new();
    let run_id = Uuid::new_v4();
    let handle = registry.register(run_id);
    let mut rx = handle.subscribe();

    handle.publish(TerminalFrame {
        seq: 0,
        data: b"hello\n".to_vec(),
        ts: OffsetDateTime::now_utc(),
    });

    let frame = rx.recv().await.unwrap();
    assert_eq!(frame.data, b"hello\n");
}
```

- [ ] **Step 2: Implement `RunStreamRegistry`**

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

pub struct RunStreamHandle {
    tx: broadcast::Sender<TerminalFrame>,
    cancel_tx: watch::Sender<bool>,
    buffer: Arc<tokio::sync::Mutex<Vec<TerminalFrame>>>,
}

impl RunStreamHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<TerminalFrame> {
        self.tx.subscribe()
    }

    pub fn publish(&self, frame: TerminalFrame) {
        let _ = self.tx.send(frame.clone());
        if let Ok(mut buf) = self.buffer.try_lock() {
            buf.push(frame);
            if buf.len() > 500 {
                let drop = buf.len() - 500;
                buf.drain(0..drop);
            }
        }
    }

    pub fn buffered_tail(&self) -> Vec<TerminalFrame> {
        self.buffer.blocking_lock().clone()
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    pub fn cancelled_rx(&self) -> watch::Receiver<bool> {
        self.cancel_tx.subscribe()
    }
}

#[derive(Default)]
pub struct RunStreamRegistry {
    inner: DashMap<Uuid, Arc<RunStreamHandle>>,
}

impl RunStreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, run_id: Uuid) -> Arc<RunStreamHandle> {
        let (tx, _) = broadcast::channel(256);
        let (cancel_tx, _) = watch::channel(false);
        let handle = Arc::new(RunStreamHandle {
            tx,
            cancel_tx,
            buffer: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        });
        self.inner.insert(run_id, handle.clone());
        handle
    }

    pub fn get(&self, run_id: Uuid) -> Option<Arc<RunStreamHandle>> {
        self.inner.get(&run_id).map(|e| e.clone())
    }

    pub fn remove(&self, run_id: Uuid) {
        self.inner.remove(&run_id);
    }
}
```

Add `dashmap = "6"` to `server/Cargo.toml`.

- [ ] **Step 3: Implement `scripted_stream`**

```rust
pub const MOCK_SCRIPT: &[&str] = &[
    "Mock agent starting...\n",
    "Reading .agent/context.md\n",
    "Running tests...\n",
    "Done.\n",
];

pub async fn emit_script(
    handle: &RunStreamHandle,
    cancel_rx: &mut watch::Receiver<bool>,
    lines: &[&str],
) {
    let mut seq = 0u64;
    for line in lines {
        if *cancel_rx.borrow() {
            break;
        }
        handle.publish(TerminalFrame {
            seq,
            data: line.as_bytes().to_vec(),
            ts: OffsetDateTime::now_utc(),
        });
        seq += 1;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p coppice-server run_registry scripted
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/sessions/run_registry.rs server/src/sessions/scripted_stream.rs server/Cargo.toml server/src/sessions/mod.rs
git commit -m "feat(server): add per-run stream registry and scripted emitter"
```

---

### Task 5: Event bus

**Files:**
- Create: `server/src/events/bus.rs`
- Create: `server/src/events/mod.rs`
- Modify: `server/src/lib.rs`

- [ ] **Step 1: Define event types**

```rust
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    TicketUpdated {
        ticket_id: Uuid,
        status: String,
        substatus: Option<String>,
        updated_at: String,
    },
    AgentRunStarted {
        run_id: Uuid,
        ticket_id: Uuid,
        agent_id: Uuid,
        status: String,
    },
    AgentRunFinished {
        run_id: Uuid,
        ticket_id: Uuid,
        agent_id: Uuid,
        status: String,
        error_message: Option<String>,
    },
    CommentCreated {
        comment_id: Uuid,
        ticket_id: Uuid,
        author_type: String,
    },
}

pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 2: Unit test serialization**

```rust
#[test]
fn agent_run_finished_serializes() {
    let event = AppEvent::AgentRunFinished {
        run_id: Uuid::nil(),
        ticket_id: Uuid::nil(),
        agent_id: Uuid::nil(),
        status: "succeeded".into(),
        error_message: None,
    };
    let raw = serde_json::to_string(&event).unwrap();
    assert!(raw.contains("agent_run.finished"));
}
```

Use `#[serde(tag = "type")]` with explicit rename if needed:

```rust
#[serde(rename = "agent_run.finished")]
AgentRunFinished { ... }
```

Adjust enum variants to match wire names from spec (`agent_run.started`, etc.) using `#[serde(rename = "...")]` on each variant.

- [ ] **Step 3: Commit**

```bash
git add server/src/events server/src/lib.rs
git commit -m "feat(server): add in-process event bus for WebSocket fan-out"
```

---

### Task 6: Extend providers + MockProvider streaming

**Files:**
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/providers/mock.rs`

- [ ] **Step 1: Extend `AgentRunInput`**

```rust
use std::sync::Arc;
use crate::sessions::run_registry::RunStreamHandle;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct AgentRunInput {
    pub agent_id: String,
    pub ticket_id: Option<String>,
    pub context_path: String,
    pub run_id: Option<String>,
    pub artifacts_dir: Option<String>,
    pub stream: Option<Arc<RunStreamHandle>>,
    pub cancel_rx: Option<watch::Receiver<bool>>,
}
```

Update all `AgentRunInput { ... }` call sites (worker, tests) to pass `stream: None, cancel_rx: None` initially — Task 7 fills them in.

- [ ] **Step 2: Update MockProvider**

```rust
async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
    if let (Some(stream), Some(mut cancel_rx)) = (&input.stream, input.cancel_rx) {
        crate::sessions::scripted_stream::emit_script(
            stream,
            &mut cancel_rx,
            crate::sessions::scripted_stream::MOCK_SCRIPT,
        )
        .await;
        if *cancel_rx.borrow() {
            return Err(ProviderError::Cancelled);
        }
    }

    // existing fixture load...
}
```

Add `ProviderError::Cancelled` variant.

- [ ] **Step 3: Run provider tests**

```bash
cargo test -p coppice-server providers::mock
```

Expected: PASS (update test struct literals with `stream: None, cancel_rx: None`)

- [ ] **Step 4: Commit**

```bash
git add server/src/providers/mod.rs server/src/providers/mock.rs
git commit -m "feat(server): stream mock provider output through run registry"
```

---

### Task 7: Wire worker — streams, artifacts, events

**Files:**
- Modify: `server/src/lib.rs`
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/tests/common/mod.rs` (test `AppState` helpers)

- [ ] **Step 1: Extend `AppState`**

```rust
pub struct AppState {
    pub config: AppConfig,
    pub db: Option<PgPool>,
    pub attachments: AttachmentStore,
    pub agent_provider: Arc<dyn crate::providers::AgentProvider>,
    pub run_streams: Arc<crate::sessions::run_registry::RunStreamRegistry>,
    pub event_bus: Arc<crate::events::bus::EventBus>,
}
```

Update `test_state()` and `main.rs` to initialize both.

- [ ] **Step 2: Update `execute_job`**

Before `agent_provider.run`:

```rust
let stream = state.run_streams.register(run.id);
let cancel_rx = stream.cancelled_rx();

state.event_bus.publish(AppEvent::AgentRunStarted {
    run_id: run.id,
    ticket_id: run.ticket_id,
    agent_id: run.agent_id,
    status: "running".into(),
});

let result = state.agent_provider.run(AgentRunInput {
    // ...existing fields...
    stream: Some(stream.clone()),
    cancel_rx: Some(cancel_rx),
}).await;
```

After run (success or before apply), persist artifacts:

```rust
let paths = RunArtifactPaths::new(&state.config.storage.artifacts_dir, &run.id.to_string());
let mut log_bytes = Vec::new();
for frame in stream.buffered_tail() {
    log_bytes.extend_from_slice(&frame.data);
}
ArtifactService::write_terminal_log(&paths, &log_bytes)?;
ArtifactService::write_meta(&paths, &RunArtifactMeta {
    provider: state.agent_provider.id().into(),
    session_id: None,
    frame_count: stream.buffered_tail().len() as u64,
    ended_at: time::OffsetDateTime::now_utc().format(&Rfc3339)?,
})?;
state.run_streams.remove(run.id);
```

On `finish_with_apply` / `finish_failed` / cancel, publish `AgentRunFinished` with final status.

- [ ] **Step 3: Emit `ticket.updated` from `result_contract` apply path**

In `run_service.finish_with_apply` (or worker after apply), publish `TicketUpdated` with new status.

- [ ] **Step 4: Emit `comment.created` in `CommentService::create`**

When comment created, `state.event_bus.publish(CommentCreated { ... })` — pass `EventBus` via `AppState` clone into service call from worker, or publish from worker after comment creation.

- [ ] **Step 5: Run integration agent tests**

```bash
cargo test -p coppice-server --test integration_agent_runs
```

Expected: PASS (may need to wait slightly longer for mock sleep — increase poll timeout if needed)

- [ ] **Step 6: Commit**

```bash
git add server/src/lib.rs server/src/main.rs server/src/workers/job_worker.rs server/tests/common/mod.rs server/src/services/run_service.rs
git commit -m "feat(server): wire run streams, artifacts, and lifecycle events in worker"
```

---

### Task 8: Stop cancels active stream

**Files:**
- Modify: `server/src/api/agent_runs.rs`
- Modify: `server/src/services/run_service.rs`

- [ ] **Step 1: Cancel stream on stop**

In `stop_run` handler after `service.stop`:

```rust
if let Some(handle) = state.run_streams.get(run_id) {
    handle.cancel();
}
```

- [ ] **Step 2: Worker respects cancel during mock sleep**

Already handled via `cancel_rx` in Task 6; verify `JobCancelled` path publishes `AgentRunFinished` with `cancelled`.

- [ ] **Step 3: Integration test stop mid-run**

Add to `server/tests/integration_live_console.rs` (created Task 11) or extend `integration_agent_runs.rs`:

- Start run
- Immediately POST stop
- Poll until `cancelled`

- [ ] **Step 4: Commit**

```bash
git add server/src/api/agent_runs.rs server/src/workers/job_worker.rs
git commit -m "feat(server): stop run cancels active stream session"
```

---

### Task 9: WebSocket endpoints

**Files:**
- Create: `server/src/api/ws/mod.rs`
- Create: `server/src/api/ws/live.rs`
- Create: `server/src/api/ws/events.rs`
- Modify: `server/src/api/mod.rs`
- Modify: `server/Cargo.toml`

- [ ] **Step 1: Enable Axum WebSocket**

`server/Cargo.toml`:

```toml
axum = { version = "0.8", features = ["multipart", "ws"] }
```

- [ ] **Step 2: Session auth helper for WS upgrade**

Create `server/src/api/ws/auth.rs`:

```rust
pub async fn auth_user_from_cookie(
    state: &AppState,
    cookies: &str,
) -> Result<AuthUser, ()> {
    let token = parse_session_cookie(cookies).ok_or(())?;
    let pool = state.db.as_ref().ok_or(())?;
    let auth = AuthService::new(pool, &state.config.auth);
    let (user, session) = auth.user_by_session_token(&token).await.map_err(|_| ())?;
    Ok(AuthUser { user, session })
}
```

Reject upgrade with HTTP 401 if auth fails.

- [ ] **Step 3: Live WS handler**

`GET /ws/agent-runs/{run_id}/live`:

1. Authenticate cookie from upgrade request headers.
2. `registry.get(run_id)` — if none, still accept connection but send buffered empty until run starts OR close with message if run already terminal (load from DB).
3. Send `buffered_tail()` frames as JSON.
4. `while let Ok(frame) = rx.recv().await { send frame }` until `end` message or run removed.
5. On stream end, send `TerminalFrame::end_message(status)`.

- [ ] **Step 4: Events WS handler**

`GET /ws/events`:

1. Authenticate.
2. Subscribe to `event_bus`.
3. Forward each `AppEvent` as JSON text message.

- [ ] **Step 5: Mount routes without CSRF**

In `server/src/api/mod.rs`:

```rust
let ws = ws::routes().layer(middleware::from_fn_with_state(
    state.clone(),
    ws_session_middleware, // sets AuthUser or rejects before upgrade
));

Router::new()
    .merge(public)
    .merge(protected)
    .merge(ws)
    .with_state(state)
```

WebSocket routes must NOT use CSRF middleware (no mutation via WS in M04).

- [ ] **Step 6: Commit**

```bash
git add server/src/api/ws server/src/api/mod.rs server/Cargo.toml
git commit -m "feat(server): add authenticated live and events WebSocket endpoints"
```

---

### Task 10: OpenCode serve manager + provider

**Files:**
- Create: `server/src/sessions/opencode_serve.rs`
- Create: `server/src/sessions/opencode_events.rs`
- Create: `server/src/providers/opencode.rs`
- Create: `fixtures/opencode-events/sample.jsonl`
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/Cargo.toml`

- [ ] **Step 1: Add `reqwest` dependency**

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

- [ ] **Step 2: Implement serve manager**

`OpenCodeServeManager`:

```rust
pub struct OpenCodeServeManager {
    child: tokio::sync::Mutex<Option<tokio::process::Child>>,
    base_url: String,
}

impl OpenCodeServeManager {
    pub async fn start(config: &OpenCodeProviderConfig) -> anyhow::Result<Arc<Self>> {
        let mut child = tokio::process::Command::new(&config.command)
            .args([
                "serve",
                "--hostname",
                &config.serve_hostname,
                "--port",
                &config.serve_port.to_string(),
            ])
            .spawn()?;
        let base_url = format!("http://{}:{}", config.serve_hostname, config.serve_port);
        // poll GET {base_url}/doc up to 30s
        Ok(Arc::new(Self {
            child: tokio::sync::Mutex::new(Some(child)),
            base_url,
        }))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn shutdown(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }
}
```

- [ ] **Step 3: Event line parser**

`fixtures/opencode-events/sample.jsonl` — 3–5 JSON lines resembling OpenCode stdout events.

`opencode_events.rs`:

```rust
pub fn event_line_to_frame(seq: u64, line: &str) -> Option<TerminalFrame> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = value.get("text").or_else(|| value.get("content"))
        .and_then(|v| v.as_str())?;
    Some(TerminalFrame {
        seq,
        data: format!("{text}\n").into_bytes(),
        ts: OffsetDateTime::now_utc(),
    })
}

pub fn extract_result_from_events(lines: &[String]) -> Option<AgentRunResult> {
    // scan lines/newlines for last JSON object containing "status":"done" or "blocked"
    for line in lines.iter().rev() {
        if let Ok(result) = serde_json::from_str::<AgentRunResult>(line) {
            return Some(result);
        }
        // also try to find ```json block in text fields
    }
    None
}
```

- [ ] **Step 4: OpenCodeProvider**

```rust
pub struct OpenCodeProvider {
    serve: Arc<OpenCodeServeManager>,
    config: OpenCodeProviderConfig,
}

#[async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str { "opencode" }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let worktree = PathBuf::from(&input.context_path)
            .parent().and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let mut args = vec![
            "run".into(),
            "--attach".into(),
            self.serve.base_url().into(),
            "--dir".into(),
            worktree.display().to_string(),
            "-p".into(),
            "Read .agent/context.md and return the Expected output contract JSON.".into(),
        ];
        if let Some(model) = &self.config.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        let mut child = tokio::process::Command::new(&self.config.command)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // read stdout lines, publish frames to input.stream, collect for result parse
        // if cancel_rx fires, kill child

        let parsed = extract_result_from_events(&collected_lines)
            .ok_or_else(|| ProviderError::InvalidFixture("no result contract in opencode output".into()))?;
        Ok(parsed)
    }
}
```

- [ ] **Step 5: Wire `agent_provider_from_config`**

```rust
match config.agent.default_provider.as_str() {
    "mock" => Arc::new(MockProvider::default()),
    "opencode" => {
        let serve = state.opencode_serve.clone()
            .expect("opencode serve not started");
        Arc::new(OpenCodeProvider::new(serve, config.agent.providers.opencode.clone()))
    }
    other => panic!("unknown agent provider: {other}"),
}
```

Refactor factory to accept optional `Arc<OpenCodeServeManager>`.

`main.rs`:

```rust
let opencode_serve = if config.agent.providers.opencode.enabled
    || config.agent.default_provider == "opencode"
{
    Some(OpenCodeServeManager::start(&config.agent.providers.opencode).await?)
} else {
    None
};
```

Register `tokio::spawn` shutdown hook or use `ctrl_c` handler calling `serve.shutdown().await`.

- [ ] **Step 6: Unit tests for event parser**

```bash
cargo test -p coppice-server opencode_events
```

Expected: PASS (no network)

- [ ] **Step 7: Commit**

```bash
git add server/src/sessions/opencode_serve.rs server/src/sessions/opencode_events.rs server/src/providers/opencode.rs fixtures/opencode-events server/src/lib.rs server/src/main.rs server/Cargo.toml
git commit -m "feat(server): add OpenCode provider with auto-started serve"
```

---

### Task 11: Integration tests for live console

**Files:**
- Create: `server/tests/integration_live_console.rs`
- Modify: `server/Cargo.toml` (dev-dep `tokio-tungstenite`)

- [ ] **Step 1: Add WS test client dep**

```toml
[dev-dependencies]
tokio-tungstenite = { version = "0.26", features = ["rustls-tls-webpki-roots"] }
futures-util = "0.3"
```

- [ ] **Step 2: Write integration test**

```rust
#[tokio::test]
async fn live_ws_receives_mock_frames_and_terminal_log_written() {
    let (app, pool, cookie, csrf) = common::setup().await;
    let repo_id = common::register_temp_repo(&app, &cookie, &csrf).await;
    let (ticket_id, _, _) = setup_agent_ticket(&app, &cookie, &csrf, &repo_id).await;

    let (status, body) = post_run_agent(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(status, StatusCode::CREATED);
    let run_id = body.unwrap()["run"]["id"].as_str().unwrap().to_string();

    let ws_url = format!("ws://127.0.0.1/ws/agent-runs/{run_id}/live");
    // connect with tokio-tungstenite, cookie header
    // recv until at least one {"type":"frame"} with "Mock agent"
    // wait for run succeeded via poll
    // assert terminal.log exists under temp artifacts dir from test config
}

#[tokio::test]
async fn ws_rejects_unauthenticated() {
    // upgrade without cookie → 401
}

#[tokio::test]
async fn events_ws_receives_run_finished() {
    // connect /ws/events, run agent, receive agent_run.finished
}
```

Use `axum::serve` with random port in test helper, or existing `common::setup()` pattern if it exposes TCP port. If `common::setup()` uses `oneshot` only, add `common::spawn_test_server()` returning `SocketAddr` for WS tests.

- [ ] **Step 3: Run integration tests**

```bash
cargo test -p coppice-server --test integration_live_console
```

Expected: PASS (requires Postgres via test harness)

- [ ] **Step 4: Commit**

```bash
git add server/tests/integration_live_console.rs server/tests/common/mod.rs server/Cargo.toml
git commit -m "test(server): add live console WebSocket integration tests"
```

---

### Task 12: Frontend — xterm Live Console tab

**Files:**
- Modify: `web/package.json`
- Create: `web/src/features/runs/LiveConsole.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/src/lib/schemas/agentRun.ts` (add `sessionId` if exposed)

- [ ] **Step 1: Install xterm**

```bash
cd web && yarn add xterm @xterm/addon-fit
```

- [ ] **Step 2: Implement `LiveConsole.tsx`**

```tsx
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from 'xterm';
import { useEffect, useRef, useState } from 'react';
import 'xterm/css/xterm.css';

interface LiveConsoleProps {
  runId: string | null;
  runStatus: string | null;
}

export function LiveConsole({ runId, runStatus }: LiveConsoleProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [connection, setConnection] = useState<'connecting' | 'open' | 'closed'>('closed');

  useEffect(() => {
    if (!containerRef.current) return;
    const term = new Terminal({ fontFamily: 'ui-monospace, monospace', fontSize: 13, scrollback: 5000 });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;
    return () => {
      term.dispose();
      termRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!runId || !termRef.current) return;
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${protocol}://${window.location.host}/ws/agent-runs/${runId}/live`);
    setConnection('connecting');

    ws.onopen = () => setConnection('open');
    ws.onclose = () => setConnection('closed');
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string);
      if (msg.type === 'frame' && typeof msg.data === 'string') {
        termRef.current?.write(msg.data);
      }
      if (msg.type === 'end') {
        ws.close();
      }
    };

    return () => ws.close();
  }, [runId]);

  // reconnect when runStatus === 'running' && connection === 'closed'

  return (
    <div className="flex h-full min-h-[320px] flex-col gap-2">
      <p className="font-body text-xs text-text-secondary">
        {connection === 'open' ? 'Live' : connection === 'connecting' ? 'Connecting…' : 'Disconnected'}
      </p>
      <div ref={containerRef} className="min-h-[280px] flex-1 overflow-hidden rounded-md border border-border bg-[#1e1e1e] p-1" />
    </div>
  );
}
```

- [ ] **Step 3: Add drawer tab**

In `TicketDrawer.tsx`:

```typescript
type DrawerTab = 'detail' | 'live' | 'runs';

const TAB_LABELS = { detail: 'Detail', live: 'Live Console', runs: 'Agent Runs' };
const TAB_ORDER: DrawerTab[] = ['detail', 'live', 'runs'];
```

Render `<LiveConsole runId={activeRun?.id ?? latestRun?.id ?? null} runStatus={activeRun?.status ?? null} />` when `tab === 'live'`.

- [ ] **Step 4: Run web tests**

```bash
make web-test
```

Update `TicketDrawer.test.tsx` if tab count assertions break.

- [ ] **Step 5: Commit**

```bash
git add web/package.json web/yarn.lock web/src/features/runs/LiveConsole.tsx web/src/features/tickets/TicketDrawer.tsx
git commit -m "feat(web): add Live Console tab with xterm.js streaming"
```

---

### Task 13: Event socket, toasts, board badge

**Files:**
- Create: `web/src/features/ws/useEventSocket.ts`
- Modify: `web/src/components/ToastProvider.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/features/board/TicketCard.tsx`
- Modify: `web/src/features/board/BoardPage.tsx`

- [ ] **Step 1: Extend ToastProvider for persistent errors**

Add optional `persistent` flag and `onClick` callback:

```typescript
interface ToastItem {
  id: string;
  message: string;
  variant: ToastVariant;
  persistent?: boolean;
  onClick?: () => void;
}

error: (message: string, opts?: { persistent?: boolean; onClick?: () => void }) => void;
```

Skip auto-dismiss timer when `persistent` is true.

- [ ] **Step 2: Implement `useEventSocket`**

```typescript
export function useEventSocket(opts: {
  enabled: boolean;
  onRunFinished?: (payload: AgentRunFinishedPayload) => void;
}) {
  useEffect(() => {
    if (!opts.enabled) return;
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${protocol}://${window.location.host}/ws/events`);

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data as string);
      if (msg.type === 'agent_run.finished') {
        opts.onRunFinished?.(msg);
      }
      if (msg.type === 'ticket.updated') {
        void queryClient.invalidateQueries({ queryKey: ['tickets'] });
      }
      if (msg.type === 'comment.created') {
        void queryClient.invalidateQueries({ queryKey: ['comments', msg.ticketId] });
      }
    };

    return () => ws.close();
  }, [opts.enabled]);
}
```

- [ ] **Step 3: Mount in `App.tsx`**

Inside `AuthProvider`, create `EventSocketBridge` component that reads auth session and calls `useToast()`:

```typescript
function EventSocketBridge() {
  const { user } = useAuth();
  const toast = useToast();
  const navigate = useNavigate();

  useEventSocket({
    enabled: Boolean(user),
    onRunFinished: (payload) => {
      if (payload.status === 'succeeded' || payload.status === 'blocked') {
        toast.success(`Agent run ${payload.status}`);
      } else {
        toast.error(`Agent run ${payload.status}`, {
          persistent: true,
          onClick: () => navigate(`/projects/...?ticket=${payload.ticketId}&tab=runs&run=${payload.runId}`),
        });
      }
    },
  });
  return null;
}
```

Use existing drawer open mechanism (board query param) instead of route navigation if that is how tickets open today — check `BoardPage` for `?ticket=` pattern and match it.

- [ ] **Step 4: Board live badge**

Track `runningTicketIds: Set<string>` in `BoardPage` from `/ws/events` `agent_run.started` / `agent_run.finished`.

Pass `isLive={runningTicketIds.has(ticket.id)}` to `TicketCard`.

Add pulsing dot when `isLive`:

```tsx
{isLive && (
  <span className="mr-1.5 inline-block h-2 w-2 animate-pulse rounded-full bg-accent" aria-label="Agent running" />
)}
```

- [ ] **Step 5: Commit**

```bash
git add web/src/features/ws/useEventSocket.ts web/src/components/ToastProvider.tsx web/src/App.tsx web/src/features/board/TicketCard.tsx web/src/features/board/BoardPage.tsx
git commit -m "feat(web): add event WebSocket, run toasts, and board live badge"
```

---

### Task 14: E2E smoke + docs

**Files:**
- Create: `e2e/smoke/m04-live-console.mjs`
- Modify: `Makefile`
- Modify: `docs/superpowers/specs/2026-06-08-m04-live-console-design.md` (status → Approved)

- [ ] **Step 1: Write smoke test**

Extend m03 flow:

```javascript
import WebSocket from 'ws'; // or use undici ws if Node 22+

async function assertLiveFrames(runId, cookie) {
  const ws = new WebSocket(`ws://localhost:8080/ws/agent-runs/${runId}/live`, {
    headers: { cookie },
  });
  const deadline = Date.now() + 15_000;
  let sawMock = false;
  await new Promise((resolve, reject) => {
    ws.on('message', (data) => {
      const msg = JSON.parse(data.toString());
      if (msg.type === 'frame' && msg.data.includes('Mock agent')) {
        sawMock = true;
      }
      if (msg.type === 'end') resolve();
    });
    ws.on('error', reject);
    const timer = setInterval(() => {
      if (Date.now() > deadline) {
        clearInterval(timer);
        reject(new Error('live ws timeout'));
      }
      if (sawMock) { /* keep waiting for end */ }
    }, 200);
  });
  if (!sawMock) fail('live stream missing mock output');
}
```

If Node built-in WebSocket is available (Node 22+), use `globalThis.WebSocket` with cookie limitation — prefer `ws` package in e2e or poll HTTP artifact endpoint if added. Simplest M04 smoke: use `ws` npm package at repo root or inline HTTP check that `terminal.log` exists via future GET artifact API. **Pragmatic smoke for M04:** after run succeeds, verify `agent_run.finished` event via second WS on `/ws/events` OR add `GET /api/agent-runs/:id/artifacts` returning paths — if not in scope, smoke asserts WS frames only using `ws` package:

```bash
npm install --no-save ws
```

at start of smoke script.

- [ ] **Step 2: Makefile target**

```makefile
e2e-smoke-m04: compose-up
	$(COMPOSE) exec -T server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
	node e2e/smoke/m04-live-console.mjs
```

- [ ] **Step 3: Update spec status**

Change design spec `Status: Draft — pending review` → `Status: Approved`.

- [ ] **Step 4: Run smoke**

```bash
make e2e-smoke-m04
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add e2e/smoke/m04-live-console.mjs Makefile docs/superpowers/specs/2026-06-08-m04-live-console-design.md
git commit -m "test(e2e): add M04 live console smoke and mark spec approved"
```

---

### Task 15: tmux stub + final verification

**Files:**
- Create: `server/src/sessions/tmux_stream.rs`
- Modify: `docs/providers.md` (confirm future TODO present)

- [ ] **Step 1: Add stub module**

```rust
//! Future CliTmuxProvider backend for Claude Code / Codex.
//! See docs/providers.md.

pub struct TmuxStream;

impl TmuxStream {
    pub fn not_implemented() -> ! {
        unimplemented!("CliTmuxProvider is documented for post-M04")
    }
}
```

- [ ] **Step 2: Full workspace checks**

```bash
make test
make clippy
make web-test
```

Expected: all PASS

- [ ] **Step 3: Commit**

```bash
git add server/src/sessions/tmux_stream.rs server/src/sessions/mod.rs
git commit -m "chore(server): add tmux stream stub for future CLI providers"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| ScriptedStream mock output | 4, 6, 7 |
| OpenCode auto serve | 10 |
| `GET /ws/agent-runs/:id/live` | 9 |
| `GET /ws/events` | 5, 9, 13 |
| Live Console xterm tab | 12 |
| Stop kills session | 8 |
| terminal.log + meta.json | 3, 7 |
| Run completion toasts | 13 |
| Board live badge | 13 |
| WS auth | 9 |
| session_id column | 1 |
| OpenCode manual only | 10 (not in CI) |
| docs/providers.md | already committed; Task 15 confirms |
| Claude/Codex deferred | 15 stub + providers.md |

## Manual verification (operator)

1. Copy `config.example.toml` → `config.toml`, set:

```toml
[agent]
default_provider = "opencode"

[agent.providers.opencode]
enabled = true
```

2. Run `opencode auth login` on host.
3. `make compose-local-up && make migrate && make bootstrap`
4. `make server-dev` and `make web-dev`
5. Run agent on ticket → Live Console shows OpenCode output → ticket updates.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-08-m04-live-console.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — implement tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
