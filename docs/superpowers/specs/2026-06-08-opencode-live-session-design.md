# OpenCode Live Session View — Design Spec

**Date:** 2026-06-08  
**Status:** Approved (brainstorming)  
**Supersedes (partial):** M04 Live Console ANSI/xterm path for OpenCode runs — see `docs/superpowers/specs/2026-06-08-m04-live-console-design.md`  
**Depends on:** M04 (WebSocket live endpoint, `session_id` column, OpenCode HTTP client, artifacts dir)  
**Upstream reference:** [anomalyco/opencode](https://github.com/anomalyco/opencode) — pin commit in `web/src/opencode-session/README.md`

## Purpose

Replace the xterm.js + ANSI frame pipeline for OpenCode runs with a **structured live session view** that matches OpenCode CLI quality: smooth per-part streaming, readable tool UI (no `...` fallbacks), reasoning blocks, and markdown text rendering.

The implementation is a **standalone, upstream-shaped React module** (`web/src/opencode-session/`) ported 1:1 from OpenCode's TUI session view so future OpenCode updates are easy to diff and merge.

Also fix **server restart recovery**: when Coppice restarts mid-run, the UI must not spin forever on "Connecting…"; it should replay persisted state and re-attach to the OpenCode session when possible.

---

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Rendering approach | **Path 1** — React port of OpenCode session view (not xterm, not OpenTUI WASM) |
| Module structure | Standalone `web/src/opencode-session/`, directory layout mirrors upstream TUI paths |
| Port fidelity | **1:1** from `packages/opencode/src/cli/cmd/tui/routes/session/` + tool components (~2000 LOC acceptable) |
| Wire protocol | Relay **raw OpenCode SSE events** (+ snapshot), not `TerminalFrame` ANSI bytes |
| Server ANSI mapper | **Remove** `opencode_stream.rs` once structured path ships |
| Recovery strategy | **Re-attach (B)** — persist `session_id` early, re-subscribe to OpenCode `/event` after Coppice restart; periodic snapshot artifact as fallback |
| Orphan runs on boot | Attempt re-attach; if OpenCode session dead → mark `interrupted` |
| Mock provider | Keep existing `ScriptedStream` → xterm path for CI, or migrate mock to synthetic events later (out of scope for v1) |
| `terminal.log` | Deprecated for OpenCode; primary artifact is `session.snapshot.json` |

---

## Out of scope

- OpenTUI WASM embedding in browser
- Full OpenCode TUI chrome (sidebar, keyboard shortcuts, scroll acceleration, copy-on-select)
- Permission prompt UI (display tool state from events if present; no interactive approve/deny in v1)
- Durable append-only event log on disk (approach C) — deferred unless re-attach proves insufficient
- Claude Code / Codex providers
- Replacing MockProvider live view in v1 (mock may keep xterm or show placeholder)

---

## Problem statement

### Current behavior (unsatisfactory)

1. **Chunky paste** — Server converts OpenCode SSE to ANSI via `OpenCodeStreamTracker` + 1.5s message poller; large suffixes arrive at once instead of smooth per-part streaming.
2. **Opaque tools** — `tool_input_summary()` only handles a few tools; others show `→ tool: ...`.
3. **Restart deadlock** — `RunStreamRegistry` is in-memory. After Coppice restart, WebSocket finds no handle, may replay empty `terminal.log`, sends `end` with `running`, and the frontend reconnects forever because DB status stays `running`.

### Target behavior

1. Text, reasoning, and tools render like OpenCode CLI: structured parts, incremental text growth, per-tool components.
2. Tool rows always show meaningful labels (command, path, pattern, task name, etc.).
3. After Coppice restart: replay snapshot → resume live SSE if OpenCode session alive → otherwise mark `interrupted` and stop reconnecting with a clear banner.

---

## Architecture overview

```text
OpenCode serve (SSE /event)
        │
        ▼
OpenCodeClient (Rust) ──publish──► RunEventRegistry (broadcast raw JSON events)
        │                                    │
        │                                    ├──► WebSocket /ws/agent-runs/:id/live
        │                                    └──► periodic session.snapshot.json write
        ▼
   session_id ──► agent_runs.session_id (DB, written at session creation)

Browser:
  LiveSession.tsx ──WS──► opencode-session/sync/reduce-event.ts ──► SessionView.tsx
```

### Why not xterm

xterm paints a character grid. OpenCode CLI maintains a **part model** (text / reasoning / tool) and renders each part with dedicated components (`TextPart`, `Bash`, `Read`, …). Matching OpenCode UX requires the same abstraction on the web.

---

## Frontend: standalone module

### Package layout

```text
web/src/opencode-session/
  README.md                     # upstream path map + pinned opencode commit
  sync/
    types.ts                    # Part, Message, Session shapes (from @opencode-ai/sdk or copied)
    store.ts                    # reactive part registry (from tui/context/sync.tsx)
    reduce-event.ts             # SSE event → store mutations (from tui/context/sdk.tsx)
  session/
    SessionView.tsx             # from routes/session/index.tsx (trimmed)
    AssistantMessage.tsx
    UserMessage.tsx
  parts/
    TextPart.tsx
    ReasoningPart.tsx
    ToolPart.tsx
  tools/
    Bash.tsx, Read.tsx, Write.tsx, Edit.tsx, Grep.tsx, Glob.tsx,
    List.tsx, WebFetch.tsx, Task.tsx, Skill.tsx, Question.tsx, ...
  theme/
    session-theme.ts            # OpenCode theme tokens → Coppice CSS variables
```

### Coppice glue (thin)

```text
web/src/features/runs/LiveSession.tsx   # WS client, run status, mounts SessionView
web/src/features/tickets/TicketDrawer.tsx   # swap LiveConsole → LiveSession for opencode runs
```

### Porting rules

1. **Mirror upstream file paths** in README so `git diff` against opencode is mechanical.
2. **Minimize Coppice imports** inside `opencode-session/` — only theme tokens and generic utilities.
3. **Solid.js → React** translation: `createMemo` → `useMemo`, `For` → `.map`, `Show` → conditional render. Logic stays identical.
4. Pin upstream commit hash in README; document delta when Coppice intentionally diverges.

### Deferred from upstream session view

- Sidebar, session list navigation, frecency, dialog/command palette
- `showDetails` toggle keyboard binding (default: show tool details)
- Animations, custom scroll acceleration
- Plugin slots

---

## WebSocket protocol

Replaces `{type: "frame", data: "<ansi>"}` for OpenCode runs.

### Server → client messages

| `type` | Payload | When |
|--------|---------|------|
| `snapshot` | `{ messages: Message[], parts: Record<messageId, Part[]> }` | On connect — from in-memory store, artifact file, or OpenCode `GET /session/{id}/message` |
| `event` | `{ event: <raw OpenCode SSE JSON> }` | Live stream |
| `end` | `{ status, reason?, recoverable: bool }` | Run finished, unrecoverable, or interrupted |

### Client behavior

1. Apply `snapshot` to seed store.
2. Apply each `event` through `reduce-event.ts` (same semantics as OpenCode TUI).
3. On `end` with `recoverable: false` **or** terminal run status from REST poll → **stop** reconnect loop.
4. Show status banner: Live / Reconnecting / Finished / Interrupted.

### Mock provider (unchanged for v1)

Mock runs may continue using the existing xterm `LiveConsole` and `TerminalFrame` protocol until a follow-up migrates mock to synthetic `event` messages.

---

## Event reducer (critical semantics)

Port logic from OpenCode `sdk.tsx` + `sync.tsx`:

1. **Register parts on `message.part.updated`** before applying deltas.
2. **`message.part.delta`** — append to part field (`text`, etc.) by `partID`; handle delta-before-updated race (OpenCode issue #26924).
3. **`message.part.updated`** — upsert full part; for text parts already streamed via deltas, do not re-emit full text.
4. **Session status events** — update running/idle indicator; do not clear parts.
5. **Tool parts** — update `state.status`, `state.input`, `state.output`; `ToolPart` component handles display.

Unit tests in `reduce-event.test.ts` cover: incremental deltas, duplicate updated, delta-before-updated, tool running→completed.

---

## Server changes

### RunEventRegistry (rename / extend RunStreamRegistry)

```rust
pub struct RunEventHandle {
    tx: broadcast::Sender<RunEvent>,           // enum: Snapshot | Event(Value) | End
    buffer: Arc<Mutex<Vec<RunEvent>>>,        // ring buffer for WS tail replay
    snapshot: Arc<Mutex<SessionSnapshot>>,   // latest merged state for periodic persist
    cancel_tx: watch::Sender<bool>,
}
```

`OpenCodeClient`:
- Remove `OpenCodeStreamTracker` usage.
- On each SSE line: `handle.publish(RunEvent::Event(json))` + update in-memory snapshot.
- Delete `opencode_stream.rs` and message poller suffix publishing (keep optional `GET /message` poll only for snapshot refresh on reconnect, not for ANSI).

### Persist `session_id` early

In `job_worker` / `OpenCodeClient::run_session`, immediately after `create_session`:

```rust
RunService::set_session_id(pool, run_id, &session_id).await?;
```

Also write `session_id` into `meta.json` artifact (already supported).

### Snapshot artifact

Path: `{artifacts_dir}/runs/{run_id}/session.snapshot.json`

Contents: `{ session_id, messages, parts, updated_at }`

Write every **5 seconds** during active run (configurable), and on run completion. Use atomic write (temp file + rename).

### WebSocket handler (`live.rs`)

On connect for `run_id`:

1. If `RunEventHandle` exists in registry → send buffered tail (`snapshot` if available, then queued `event`s).
2. Else load `session.snapshot.json` from artifact dir → send as `snapshot`.
3. If run status is `running` or `queued` and `session_id` present:
   - Spawn re-attach task: subscribe to OpenCode `GET /event?directory=...`, filter by `sessionID`, forward as `event` messages.
   - If OpenCode returns 404 or session status is terminal → `RunService::mark_interrupted`, send `end { recoverable: false }`.
4. Else if run is terminal → send `end` with DB status only.
5. Subscribe client to live broadcast until `end`.

### Boot orphan sweep

On server startup (`main.rs` or dedicated task):

```
FOR each agent_run WHERE status IN ('running', 'queued'):
  IF no active worker lease for run:
    IF session_id AND opencode session alive:
      register re-attach listener (optional — WS clients get it on connect)
    ELSE:
      mark status = 'interrupted', error_message = 'Server restarted during run'
```

This prevents permanently stuck `running` rows.

### Run status: `interrupted`

Add `interrupted` to `RunStatus` enum (or map to `failed` with distinct `error_message`). Frontend treats `interrupted` as terminal (no reconnect).

If adding enum value is too heavy for v1, use `failed` with `error_message = "interrupted: server restarted"` and `end.recoverable = false`.

---

## Recovery flow

```text
Coppice restarts mid-run
  → FE reconnects WS
  → WS sends session.snapshot.json (last 5s state)
  → WS re-attaches OpenCode SSE for stored session_id
  → if alive: live events resume
  → if dead: DB → interrupted, WS → end(recoverable=false)
  → FE shows banner, stops 800ms reconnect loop
```

**Success criteria:** User never sees infinite "Connecting… → Disconnected — reconnecting…" after server restart.

---

## Tool display

Port per-tool components from OpenCode `session/index.tsx` (`Bash`, `Read`, `Write`, …). Each component reads `part.state.input` and `part.state.output` with the same field names as upstream.

Remove `tool_input_summary()` entirely (server-side).

Fallback for unknown tools: show tool name + pretty-printed JSON of `state.input` (truncated), never bare `...`.

---

## Text / reasoning rendering

- **TextPart:** markdown rendering (use existing Coppice markdown stack or `react-markdown` with code highlighting). `streaming={true}` behavior: re-render on each delta append.
- **ReasoningPart:** muted left-border block, prefix `_Thinking:_`, filter `[REDACTED]` chunks (OpenRouter).
- Default: show thinking (`showThinking: true`).

---

## File map (implementation)

| Path | Responsibility |
|------|----------------|
| `web/src/opencode-session/**` | Standalone ported session view |
| `web/src/features/runs/LiveSession.tsx` | WS glue, reconnect policy |
| `web/src/features/runs/LiveConsole.tsx` | Keep for mock runs only (v1) |
| `web/src/features/tickets/TicketDrawer.tsx` | Route opencode → LiveSession |
| `server/src/sessions/run_registry.rs` | `RunEvent` broadcast + snapshot buffer |
| `server/src/sessions/opencode_client.rs` | Publish raw SSE; remove ANSI tracker |
| `server/src/sessions/opencode_stream.rs` | **Delete** |
| `server/src/sessions/session_snapshot.rs` | Snapshot type + merge helpers |
| `server/src/api/ws/live.rs` | Snapshot + re-attach logic |
| `server/src/services/artifact_service.rs` | `session.snapshot.json` paths |
| `server/src/services/run_service.rs` | `set_session_id` call site; `mark_interrupted` |
| `server/src/workers/job_worker.rs` | Early session_id persist; snapshot flush on finish |
| `server/src/main.rs` | Orphan run sweep on boot |
| `server/tests/integration_live_console.rs` | Update for event protocol + recovery |
| `docs/providers/opencode.md` | Document new live view + recovery |

---

## Testing

### Unit (web)

- `reduce-event.test.ts` — delta streaming, races, tool state transitions
- Snapshot apply + incremental events produce same final state as OpenCode

### Unit (server)

- `session_snapshot.rs` — merge events into snapshot
- Orphan sweep marks dead sessions interrupted

### Integration

- WS receives `snapshot` then `event` stream during live OpenCode run (mock SSE fixture)
- Restart simulation: clear registry, connect WS, verify re-attach or `end(recoverable=false)`
- `session_id` written to DB before run completes

### Manual

- Run researcher agent; confirm smooth text streaming, readable tool labels
- Restart `coppice-server` mid-run; confirm recovery or clean interrupted state (no infinite reconnect)

---

## Migration / rollout

1. Ship `opencode-session` module + new WS protocol behind feature flag or connector check (`connector === 'opencode'` → LiveSession).
2. Wire `set_session_id` + snapshot persistence.
3. Ship recovery in same release (otherwise restart bug remains).
4. Remove `opencode_stream.rs` and xterm path for OpenCode once manual QA passes.
5. Update M04 spec with note that OpenCode live view supersedes ANSI path.

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Large port drifts from upstream | README path map + pinned commit; periodic sync |
| Delta-before-updated race | Port OpenCode reducer tests; buffer orphan deltas |
| Re-attach fails silently | Timeout + explicit `interrupted` status |
| Snapshot 5s stale on crash | Acceptable; re-attach fills gap if session alive |
| Bundle size from tool components | Code-split `opencode-session` lazy route |

---

## Acceptance criteria

- [ ] OpenCode runs render in structured session view (not xterm)
- [ ] Text streams smoothly per delta (no multi-sentence paste pauses from server poller)
- [ ] Tools show meaningful labels for all tools OpenCode renders in CLI
- [ ] `session_id` persisted at session creation
- [ ] `session.snapshot.json` written during run
- [ ] Coppice restart: no infinite reconnect; user sees interrupted or resumed stream
- [ ] Standalone module documented with upstream file mapping
