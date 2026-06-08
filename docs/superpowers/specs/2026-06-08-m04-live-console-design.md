# M04 — Live Console Design Spec

**Date:** 2026-06-08  
**Status:** Approved  
**Milestone doc:** `docs/milestones/M04-live-console.md`  
**Depends on:** M01 (auth, WebSocket session cookies), M03 (agent runs, job worker, MockProvider, result contract)  
**Product:** Coppice — grow an agent team from shared roots.

## Purpose

M04 makes agent runs observable and reactive: live terminal streaming in the browser, persisted log artifacts, realtime board updates via WebSocket, run-completion toasts, and Stop wired to real processes. CI and E2E continue using **MockProvider** only. For manual testing, Coppice adds an **OpenCode provider** that auto-starts `opencode serve` and runs jobs via `opencode run --attach`.

Claude Code and Codex are **documented but not implemented** in M04 — subscription/OAuth CLIs are harder to control programmatically and will use a future `CliTmuxProvider`.

This spec captures brainstorming decisions (2026-06-08) and refines the milestone doc with concrete architecture, provider tiers, config, and testing requirements.

---

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Live console + structured results | **Both** — tiered stream backends normalize to the same `TerminalFrame` format |
| CI / E2E | **Mock only** — never real OpenCode or other LLM providers |
| OpenCode integration | `opencode serve` **auto-started** by server; per-run `opencode run --attach` |
| OpenCode serve lifecycle (M04) | Start with server, stop with server (SIGTERM child) |
| OpenCode serve lifecycle (future) | **TODO:** idle shutdown; restart when next job is claimed |
| Manual `attach_url` override | Deferred — document as future config option |
| Claude / Codex | Document capability matrix + `CliTmuxProvider` sketch; implement post-M04 |
| Multica reference | Per-provider adapters + event-stream transcript (not tmux for API-capable tools) |
| tmux in M04 | `TmuxStream` module stubbed for future CLI providers; Mock/OpenCode use non-tmux backends |
| Toasts | Reuse existing `ToastProvider` (top-right); subscribe via `/ws/events` app-wide |

---

## Out of scope

- Workflow mentions and mention jobs (M05)
- Strict sandbox command filtering (M07)
- `CliTmuxProvider` implementation (Claude Code, Codex)
- On-demand provider process lifecycle (idle stop / job wake)
- Manual OpenCode serve attach URL
- `portable-pty` driver (future alternative to tmux)
- Cloud/daemon split execution model (Multica-style remote runtime)

---

## Architecture overview

### Tiered stream backends (selected)

M04 uses **provider-specific stream backends** that all emit the same `TerminalFrame` type. This matches Multica's per-provider adapter pattern while keeping Coppice's in-process worker model (no separate daemon binary).

| Backend | Provider | How it streams | How it stops |
|---------|----------|----------------|--------------|
| `ScriptedStream` | Mock | Pre-written chunks with small delays | Cancel flag |
| `OpenCodeStream` | OpenCode | SSE/JSON events from attached `opencode serve` | API session abort + child kill |
| `TmuxStream` | *(future)* Claude, Codex | `tmux capture-pane` polling | `tmux kill-session` |

Rejected alternatives:

- **tmux-only for all providers** — poor fit for OpenCode structured events; fights TUI capture; Multica avoids this for API-capable tools.
- **Dual tmux + API per run** — unnecessary complexity for M04; OpenCode event stream is sufficient for Live Console.

### Run pipeline (updated)

```text
POST /api/tickets/:id/run-agent
  → validate preconditions (unchanged from M03)
  → INSERT agent_run (queued) + agent_job (pending)

Worker claims job:
  → agent_run.status = running
  → emit agent_run.started on event bus
  → ensure worktree + write .agent/context.md (unchanged)
  → RunSession::start(provider backend)
      → spawn stream task → broadcast TerminalFrame to WS subscribers
  → provider.run / execute (blocks until done)
  → persist terminal.log + meta.json artifacts
  → parse AgentRunResult from provider output
  → result_contract::apply → ticket status + agent comment
  → emit agent_run.finished on event bus
  → agent_run.status = succeeded | blocked | failed
  → agent_job.status = done | failed | cancelled
```

### Stop and retry

**Stop** (`POST /api/agent-runs/:id/stop`):

- Allowed when run status is `queued` or `running` (unchanged).
- Set cancellation flag (existing M03 behavior).
- **M04:** kill active `RunSession` — OpenCode abort API, mock cancel flag, or future tmux kill.
- Close live WebSocket subscribers with a normal close code.
- Run ends as `cancelled`.

**Retry** (`POST /api/agent-runs/:id/retry`): unchanged from M03 — new run + job, same preconditions.

### Monorepo delta

```text
server/migrations/004_live_console.sql       # optional session_id column
server/src/
  sessions/
    mod.rs
    terminal_frame.rs
    run_session.rs
    scripted_stream.rs
    opencode_stream.rs
    opencode_serve.rs          # child process manager
    tmux_stream.rs             # stub for future CLI providers
  events/
    mod.rs
    bus.rs
  api/ws/
    mod.rs
    live.rs
    events.rs
  services/
    artifact_service.rs
  providers/
    opencode.rs
config/src/lib.rs                            # OpenCode provider config
docs/providers.md                            # capability matrix (incl. deferred CLIs)
web/src/features/
  runs/LiveConsole.tsx
  ws/useEventSocket.ts
e2e/smoke/m04-live-console.mjs
```

---

## Provider layer

### AgentProvider trait (unchanged surface)

The existing `AgentProvider::run(AgentRunInput) -> AgentRunResult` trait remains the orchestration boundary. M04 extends execution so the worker:

1. Starts a `RunSession` before calling `run`.
2. The provider implementation registers its stream backend with the session.
3. `run` blocks until the agent finishes; stream task runs concurrently.

### MockProvider (CI / default dev compose)

- Emits scripted terminal chunks via `ScriptedStream` (simulates typing).
- Returns fixture JSON from `fixtures/agent-responses/` (unchanged).
- Env `MOCK_AGENT_STDOUT=1` sidecar behavior superseded by live stream + `terminal.log` artifact (remove or keep as no-op compat shim during migration).
- No tmux, no network, deterministic timing for E2E.

### OpenCodeProvider (manual testing)

**Prerequisites (operator):**

- `opencode` on `PATH`
- Auth configured via `opencode auth login` (API keys stay on host; Coppice never stores them)
- Set `default_provider = "opencode"` in `config.toml` for manual runs

**Serve manager (`opencode_serve.rs`):**

- On server startup (when OpenCode provider enabled): spawn  
  `opencode serve --hostname {serve_hostname} --port {serve_port}`
- Health-check `GET http://{host}:{port}/doc` before marking ready; log warning and disable OpenCode runs if serve fails to start.
- On server shutdown: SIGTERM serve child, wait briefly, SIGKILL if needed.

**Per run:**

```text
opencode run \
  --attach http://{host}:{port} \
  --dir {worktree_path} \
  -p "Read .agent/context.md and follow the Expected output contract."
```

- Subscribe to OpenCode JSON/SSE events during run.
- Map events → `TerminalFrame` for Live Console (tool calls, assistant text, errors).
- On completion: extract final JSON object matching `AgentRunResult` contract from event stream or last assistant message.
- Store OpenCode `session_id` on `agent_runs.session_id` for debugging and future resume.

**Known pitfall (from Multica upstream):** spawning `opencode` via bare `exec` (no shell) can produce zero stdout on some versions. M04 uses `--attach` to serve, not raw TUI spawn. If direct spawn is needed later, wrap in `sh -c`.

**Config:**

```toml
[agent]
default_provider = "mock"   # CI and agent Docker stack stay mock

[agent.providers.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
# model = "..."       # optional; OpenCode default if omitted
# variant = "..."     # optional
```

### Future: on-demand serve lifecycle (TODO — document only)

```text
Future enhancement (not M04):
- After idle_timeout with no queued/running OpenCode jobs, stop serve child.
- On next job claim for OpenCode provider, restart serve and await health-check.
- Config: idle_timeout_secs, min_uptime_secs.
- Goal: free resources when agents are idle without manual serve management.
```

### Future: CliTmuxProvider (documented, not built)

See `docs/providers.md` for full capability matrix. Summary:

| Tool | Auth | Stream (future) | Session resume | Result parsing |
|------|------|-----------------|----------------|----------------|
| Claude Code | User pre-authenticates (`claude auth`) | `TmuxStream` | Supported | JSON tail from terminal log |
| Codex | User subscription / OAuth | `TmuxStream` | Unreliable | JSON tail, best-effort |
| OpenCode | API key via `opencode auth` | `OpenCodeStream` (M04) | Supported | Structured events |

Coppice does not inject subscription credentials. Operator authenticates CLIs outside the platform.

---

## Terminal streaming

### TerminalFrame

```rust
pub struct TerminalFrame {
    pub seq: u64,
    pub data: Vec<u8>,   // UTF-8 terminal bytes (may include ANSI)
    pub ts: DateTime<Utc>,
}
```

- WebSocket messages: JSON `{ "type": "frame", "seq", "data" }` where `data` is base64 or UTF-8 string (pick one in implementation; prefer UTF-8 string with JSON escaping for mock text).
- Optional final message: `{ "type": "end", "status": "succeeded" | "failed" | "cancelled" }`.

### Live WebSocket

- **Endpoint:** `GET /ws/agent-runs/:id/live`
- **Auth:** session cookie on upgrade; 401/close if unauthenticated.
- **Behavior:**
  - Subscribe to run's broadcast channel on connect.
  - Send buffered tail (last N KB from in-memory ring buffer) so reconnect shows recent output.
  - Forward frames until run ends, then send `end` and close.

### Event WebSocket

- **Endpoint:** `GET /ws/events`
- **Auth:** session cookie on upgrade.
- **Events:**

| Event | Payload (minimal) |
|-------|-------------------|
| `ticket.updated` | `{ ticketId, status, substatus?, updatedAt }` |
| `agent_run.started` | `{ runId, ticketId, agentId, status }` |
| `agent_run.finished` | `{ runId, ticketId, agentId, status, errorMessage? }` |
| `comment.created` | `{ commentId, ticketId, authorType }` |

- In-process `broadcast` channel (Tokio); fan-out per connected WS client.
- Frontend invalidates TanStack Query caches on relevant events.

---

## Data model

### Migration `004_live_console.sql`

```sql
ALTER TABLE agent_runs
  ADD COLUMN IF NOT EXISTS session_id TEXT NULL;
```

No other schema changes. `error_message` unchanged from M03.

### Artifacts

```text
{artifacts_dir}/runs/{run-id}/terminal.log    # raw frame data concatenated
{artifacts_dir}/runs/{run-id}/meta.json       # { provider, sessionId?, frameCount, endedAt }
```

- Written on run completion (success, failure, blocked, cancelled).
- Database stores metadata reference only if artifact rows are added later; M04 may write files without a DB artifact row (consistent with M03 mock stdout sidecar).

---

## Frontend

### Live Console tab

- New drawer tab: **Live Console** (alongside Detail, Agent Runs).
- `LiveConsole.tsx`: xterm.js instance, connects to `/ws/agent-runs/:id/live`.
- Show connecting / disconnected / ended states.
- Auto-scroll while at bottom; pause auto-scroll if user scrolls up.
- Reconnect on disconnect while run is still `running`.

### Board live badge

- Ticket card shows a subtle pulsing indicator when any run for that ticket is `running`.
- Clears on `agent_run.finished` via `/ws/events` (no full page reload).

### Run completion toasts

Reuse `web/src/components/ToastProvider.tsx` (already top-right).

- App-level `useEventSocket` hook mounted in `App.tsx` (inside auth boundary).
- On `agent_run.finished`:
  - `succeeded` / `blocked`: success toast, ~3s auto-dismiss.
  - `failed` / `cancelled` with error: failure toast, persistent.
  - Failure toast click: open ticket drawer → Agent Runs tab → scroll to run, brief highlight, expand error.
- Toasts fire when drawer is closed.

---

## Docker Compose delta

**Agent/CI stack (`deploy/docker-compose.yml`):**

- `default_provider` remains `mock` in `deploy/config/default.toml`.
- No `opencode` binary in server image for CI.
- Optional: install `tmux` in server image for future CLI providers (not required for M04 mock/OpenCode-on-host workflow).

**Human hot-reload stack:**

- OpenCode runs on host alongside `make server-dev`; not inside Docker.
- Serve binds `127.0.0.1` only.

---

## Testing strategy

### Unit tests

- `TerminalFrame` encoding/decoding
- Artifact path builder (`runs/{id}/terminal.log`, `meta.json`)
- Event payload serialization (`ticket.updated`, `agent_run.*`)
- OpenCode event → `TerminalFrame` mapping (fixture JSON files)
- `opencode_serve` health-check logic (mock HTTP or test double)

### Integration tests (Mock only)

- Start mock run → WS client receives ≥1 frame → run completes → `terminal.log` exists
- Stop run mid-mock → session cancelled → WS closes cleanly
- Event bus: `agent_run.started` / `agent_run.finished` received by WS subscriber
- WS rejected without session cookie
- Reconnect WS mid-run → receives buffered tail

### E2E smoke (CI)

`e2e/smoke/m04-live-console.mjs`:

1. Login → open ticket → Run Agent (mock)
2. Open Live Console tab
3. Assert terminal contains mock output text
4. Wait for run complete → Agent Runs tab shows succeeded
5. (Optional) assert toast appeared

### Manual only (not CI)

- Configure `default_provider = "opencode"`, valid `opencode auth`
- Run agent on real ticket → Live Console shows OpenCode events
- Verify result contract applied (status move + agent comment)
- Stop mid-run → run cancelled, stream ends

---

## Acceptance criteria

- [ ] Live Console displays streaming mock output during run
- [ ] Terminal log persisted as filesystem artifact (`terminal.log` + `meta.json`)
- [ ] Board updates without full page reload (`/ws/events`)
- [ ] Stop terminates active run session (mock cancel; OpenCode abort when enabled)
- [ ] WebSocket requires authentication
- [ ] CI smoke E2E passes (mock only)
- [ ] Run finished → toast appears (success and failure); failure toast navigates to Agent Runs error detail
- [ ] OpenCode provider works locally when enabled and `opencode` is authenticated (manual verification)
- [ ] `docs/providers.md` documents deferred Claude/Codex path and future idle-serve TODO

---

## References

- `docs/milestones/M04-live-console.md`
- `docs/superpowers/specs/2026-06-08-ticket-drawer-and-run-errors-design.md` (toasts deferred to M04)
- `docs/superpowers/specs/2026-06-08-m03-agent-execution-design.md`
- `docs/philosophy/final_agent_workspace_product_design.md` §10, §22
- [Multica AI coding tools matrix](https://www.multica.ai/docs/providers)
- [OpenCode server docs](https://opencode.ai/docs/server/)
