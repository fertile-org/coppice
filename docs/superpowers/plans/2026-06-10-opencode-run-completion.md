# OpenCode Run Completion Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure OpenCode agent runs finish within seconds after the model returns a `done`/`blocked` contract — updating run status, ticket status, and agent comments — instead of hanging on `running` indefinitely.

**Architecture:** Fix the post-idle deadlock in `OpenCodeClient::run_session` by signalling the SSE loop to stop when REST reports `idle`, draining the stream task with a short timeout, then extracting the result contract from API messages with a session-snapshot fallback. Relax server-side JSON deserialization to match the lenient frontend parser. Invalidate the single-ticket React Query cache when `ticket.updated` fires.

**Tech Stack:** Rust (Tokio, serde, reqwest), OpenCode HTTP/SSE API, React/TanStack Query, Vitest

**Context:** Live Session UI can show a Done card while the worker is blocked on `events_handle.await` because SSE `idle` was never observed. `finish_with_apply` never runs → run stays `running`, no comment, ticket unchanged.

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/src/providers/mod.rs` | `AgentRunResult::Done` — add `#[serde(default)]` on optional array fields |
| `server/src/sessions/opencode_events.rs` | `extract_result_from_snapshot`, minimal-json test, snapshot fallback test |
| `server/src/sessions/opencode_client.rs` | Stop SSE after idle, bounded drain, snapshot fallback extraction, shorter idle timeout |
| `server/src/sessions/session_snapshot.rs` | `messages_for_extraction()` — merge messages + parts for contract parser |
| `web/src/features/ws/useEventSocket.ts` | Invalidate `['ticket', ticketId]` on `ticket.updated` |
| `web/src/features/ws/useEventSocket.test.ts` | Unit test for ticket query invalidation |

---

### Task 1: Lenient `done` contract deserialization

**Files:**
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/sessions/opencode_events.rs`

- [ ] **Step 1: Write the failing test**

Add to `server/src/sessions/opencode_events.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn extract_result_from_minimal_done_json() {
    let minimal = r#"{"status":"done","summary":"Test complete.","nextStatus":"In Review"}"#;
    let messages = vec![serde_json::json!({
        "info": { "role": "assistant" },
        "parts": [{ "type": "text", "text": minimal }]
    })];
    let result = extract_result_from_messages(&messages).expect("minimal done should parse");
    match result {
        AgentRunResult::Done {
            summary,
            next_status,
            changed_files,
            tests_run,
            mention_agents,
            blockers,
        } => {
            assert_eq!(summary, "Test complete.");
            assert_eq!(next_status, "In Review");
            assert!(changed_files.is_empty());
            assert!(tests_run.is_empty());
            assert!(mention_agents.is_empty());
            assert!(blockers.is_empty());
        }
        _ => panic!("expected done"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-server extract_result_from_minimal_done_json -- --nocapture`

Expected: FAIL — deserialization error or `None` from `extract_result_from_messages`

- [ ] **Step 3: Add `#[serde(default)]` to Done fields**

In `server/src/providers/mod.rs`, update `AgentRunResult::Done`:

```rust
Done {
    summary: String,
    #[serde(default, rename = "changedFiles")]
    changed_files: Vec<String>,
    #[serde(default, rename = "testsRun")]
    tests_run: Vec<String>,
    #[serde(rename = "nextStatus")]
    next_status: String,
    #[serde(default, rename = "mentionAgents")]
    mention_agents: Vec<String>,
    #[serde(default)]
    blockers: Vec<String>,
},
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p coppice-server extract_result_from_minimal_done_json -- --nocapture`

Expected: PASS

- [ ] **Step 5: Run existing opencode_events tests**

Run: `cargo test -p coppice-server opencode_events -- --nocapture`

Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add server/src/providers/mod.rs server/src/sessions/opencode_events.rs
git commit -m "fix(server): accept minimal done contract JSON from OpenCode"
```

---

### Task 2: Snapshot → API message adapter for contract extraction

**Files:**
- Modify: `server/src/sessions/session_snapshot.rs`
- Modify: `server/src/sessions/opencode_events.rs`

- [ ] **Step 1: Write the failing test**

Add to `server/src/sessions/session_snapshot.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn messages_for_extraction_merges_parts() {
    let mut snap = SessionSnapshot::empty("ses_1");
    snap.messages.push(serde_json::json!({
        "id": "msg_1",
        "info": { "role": "assistant", "id": "msg_1" }
    }));
    snap.parts.insert(
        "msg_1".into(),
        vec![serde_json::json!({
            "type": "text",
            "text": r#"{"status":"done","summary":"From snapshot.","nextStatus":"In Review"}"#
        })],
    );

    let messages = snap.messages_for_extraction();
    let result = crate::sessions::opencode_events::extract_result_from_messages(&messages)
        .expect("extract from snapshot-shaped messages");
    match result {
        crate::providers::AgentRunResult::Done { summary, .. } => {
            assert_eq!(summary, "From snapshot.");
        }
        _ => panic!("expected done"),
    }
}
```

Add to `SessionSnapshot` impl in `session_snapshot.rs`:

```rust
/// Build OpenCode API-shaped messages (info + parts) for contract extraction.
pub fn messages_for_extraction(&self) -> Vec<serde_json::Value> {
    use serde_json::json;

    self.messages
        .iter()
        .filter_map(|message| {
            let message_id = message_id_from_value(message)?;
            let parts = self.parts.get(message_id)?.clone();
            let info = message
                .get("info")
                .cloned()
                .unwrap_or_else(|| message.clone());
            Some(json!({ "info": info, "parts": parts }))
        })
        .collect()
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p coppice-server messages_for_extraction_merges_parts -- --nocapture`

Expected: PASS (implementation included above)

- [ ] **Step 3: Add `extract_result_from_snapshot` helper**

In `server/src/sessions/opencode_events.rs`:

```rust
use crate::sessions::session_snapshot::SessionSnapshot;

pub fn extract_result_from_snapshot(snapshot: &SessionSnapshot) -> Option<AgentRunResult> {
    extract_result_from_messages(&snapshot.messages_for_extraction())
}
```

Add test in same file:

```rust
#[test]
fn extract_result_from_snapshot_helper() {
    let mut snap = SessionSnapshot::empty("ses_1");
    snap.messages.push(serde_json::json!({
        "id": "msg_1",
        "info": { "role": "assistant", "id": "msg_1" }
    }));
    snap.parts.insert(
        "msg_1".into(),
        vec![serde_json::json!({
            "type": "text",
            "text": r#"{"status":"done","summary":"Snap.","nextStatus":"In Review"}"#
        })],
    );
    let result = extract_result_from_snapshot(&snap).expect("snapshot extract");
    match result {
        AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Snap."),
        _ => panic!("expected done"),
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server extract_result_from_snapshot messages_for_extraction -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/sessions/session_snapshot.rs server/src/sessions/opencode_events.rs
git commit -m "feat(server): extract result contract from session snapshot"
```

---

### Task 3: Fix SSE deadlock after session idle

**Files:**
- Modify: `server/src/sessions/opencode_client.rs`

**Root cause:** `wait_idle` can return when REST `/session/status` is `idle`, but `stream_events_loop` only exits when `idle_flag` is set from SSE. `events_handle.await` then blocks forever.

- [ ] **Step 1: Add constants and drain helper**

At top of `opencode_client.rs`, replace/add:

```rust
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SSE_RECONNECT_DELAY: Duration = Duration::from_millis(750);
const RUN_IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const STREAM_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
```

Remove `RUN_TIMEOUT` (3600s) — 10 minutes is enough for a single agent turn; adjust if needed.

Add helper after `StreamContext` impl:

```rust
async fn drain_events_task(
    handle: tokio::task::JoinHandle<Result<(), ProviderError>>,
    idle_flag: &Arc<std::sync::atomic::AtomicBool>,
) {
    idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    match tokio::time::timeout(STREAM_DRAIN_TIMEOUT, handle).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(err))) => tracing::warn!(%err, "opencode event stream ended with error"),
        Ok(Err(join_err)) => tracing::warn!(%join_err, "opencode event stream task panicked"),
        Err(_) => tracing::warn!("opencode event stream drain timed out; continuing"),
    }
}
```

- [ ] **Step 2: Signal idle in `wait_idle` on REST-only path**

In `wait_idle`, change both successful return paths to set the flag:

```rust
if idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
    match self.session_status(directory, session_id).await? {
        Some(status) if status == "idle" => {
            idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            return Ok(());
        }
        _ => idle_flag.store(false, std::sync::atomic::Ordering::Relaxed),
    }
}

match self.session_status(directory, session_id).await? {
    Some(status) if status == "idle" => {
        idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }
    _ => {}
}
```

Update deadline to use `RUN_IDLE_TIMEOUT`.

- [ ] **Step 3: Use drain helper in `run_session`**

Replace the block after `wait_idle`:

```rust
let wait_result = self
    .wait_idle(&directory, &session_id, cancel_rx, idle_flag.clone())
    .await;
if let Err(err) = wait_result {
    let _ = self.abort(&directory, &session_id).await;
    drain_events_task(events_handle, &idle_flag).await;
    return Err(err);
}

drain_events_task(events_handle, &idle_flag).await;

let snapshot = ctx.snapshot.lock().map_err(|_| {
    ProviderError::InvalidFixture("snapshot lock poisoned".into())
})?;

let messages = self.fetch_messages(&directory, &session_id).await?;
extract_result_from_messages(&messages)
    .or_else(|| extract_result_from_snapshot(&snapshot))
    .ok_or_else(|| {
        ProviderError::InvalidFixture(
            "no result contract in opencode session messages or snapshot".into(),
        )
    })
```

**Important:** `ctx` must remain in scope — it is already defined before `events_handle`. Do not drop `ctx` before extraction.

Also update error paths (prompt failure, abort) to use `drain_events_task` instead of bare `events_handle.await`.

- [ ] **Step 4: Build and run server tests**

Run: `cargo test -p coppice-server -- --nocapture`

Expected: all PASS

- [ ] **Step 5: Manual smoke test**

1. Restart server: `make compose-up` or `cargo run -p coppice-server`
2. Open a test ticket ("just report dummy done")
3. Run Agent with OpenCode connector
4. Wait for Live Session Done card
5. Within ~10s, verify:
   - Agent Runs tab → `succeeded` (not `running`)
   - Comments tab → new agent comment
   - Ticket status → `in_review` (or whatever `nextStatus` was)
6. Check server logs — no hang, no 1h wait

- [ ] **Step 6: Commit**

```bash
git add server/src/sessions/opencode_client.rs
git commit -m "fix(server): stop OpenCode SSE loop after idle and extract result from snapshot fallback"
```

---

### Task 4: Regression test for REST-idle without SSE idle event

**Files:**
- Modify: `server/src/sessions/opencode_events.rs` (or new `server/src/sessions/opencode_client.rs` test module)

This task documents the scenario; full async mock of `OpenCodeClient` is heavy. Prefer a focused unit test on `wait_idle` flag behaviour via a small extracted function if needed.

- [ ] **Step 1: Add test that REST idle sets flag**

Extract from `wait_idle` the status check into a testable function in `opencode_client.rs`:

```rust
fn mark_idle_when_status(status: Option<&str>, idle_flag: &std::sync::atomic::AtomicBool) -> bool {
    if status == Some("idle") {
        idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_idle_when_status_sets_flag() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        assert!(mark_idle_when_status(Some("idle"), &flag));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mark_idle_when_status(Some("busy"), &flag));
    }
}
```

Use `mark_idle_when_status` inside `wait_idle` return paths.

- [ ] **Step 2: Run test**

Run: `cargo test -p coppice-server mark_idle_when_status -- --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add server/src/sessions/opencode_client.rs
git commit -m "test(server): cover REST idle flag signalling for OpenCode SSE shutdown"
```

---

### Task 5: Frontend — refresh ticket drawer on `ticket.updated`

**Files:**
- Create: `web/src/features/ws/useEventSocket.test.ts`
- Modify: `web/src/features/ws/useEventSocket.ts`

- [ ] **Step 1: Write the failing test**

Create `web/src/features/ws/useEventSocket.test.ts`:

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { QueryClient } from '@tanstack/react-query';

const invalidateSpy = vi.fn();

vi.mock('../../lib/query-client', () => ({
  queryClient: {
    invalidateQueries: (...args: unknown[]) => invalidateSpy(...args),
  },
}));

describe('useEventSocket dispatch', () => {
  beforeEach(() => {
    invalidateSpy.mockClear();
  });

  it('invalidates ticket and tickets queries on ticket.updated', async () => {
    const { dispatchMessageForTest } = await import('./useEventSocket');
    dispatchMessageForTest(
      JSON.stringify({
        type: 'ticket.updated',
        ticket_id: 'ticket-123',
        status: 'in_review',
      }),
    );

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['tickets'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['ticket', 'ticket-123'] });
  });
});
```

Export a test-only dispatcher from `useEventSocket.ts`:

```typescript
export function dispatchMessageForTest(raw: string) {
  dispatchMessage(raw);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- --run src/features/ws/useEventSocket.test.ts`

Expected: FAIL — `['ticket', 'ticket-123']` not invalidated

- [ ] **Step 3: Update handler**

In `useEventSocket.ts` `dispatchMessage`:

```typescript
if (msg.type === 'ticket.updated') {
  void queryClient.invalidateQueries({ queryKey: ['tickets'] });
  if (msg.ticket_id) {
    void queryClient.invalidateQueries({
      queryKey: ['ticket', msg.ticket_id],
    });
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web && npm test -- --run src/features/ws/useEventSocket.test.ts`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web/src/features/ws/useEventSocket.ts web/src/features/ws/useEventSocket.test.ts
git commit -m "fix(web): refresh open ticket drawer when ticket.updated fires"
```

---

### Task 6: End-to-end verification checklist

- [ ] **Step 1: Run full server test suite**

Run: `cargo test -p coppice-server`

Expected: PASS

- [ ] **Step 2: Run web opencode-session + ws tests**

Run: `cd web && npm test -- --run src/opencode-session src/features/ws`

Expected: PASS

- [ ] **Step 3: Manual regression on the failing scenario**

Repeat the "Just a test ticket" run:

| Check | Expected |
|-------|----------|
| Live Session Done card | Visible |
| Agent Runs status | `succeeded` within seconds |
| Agent comment | Present with summary text |
| Ticket status | Matches `nextStatus` (e.g. `in_review`) |
| Board card column | Moves without manual refresh |
| Server logs | No indefinite hang; optional drain timeout warn is OK |

- [ ] **Step 4: Verify failed-run path still works**

Run a ticket that forces failure (e.g. stop run mid-flight or invalid repo). Agent Runs should show `failed` or `cancelled` with `error_message`, not stuck `running`.

---

## Self-review

| Requirement | Task |
|-------------|------|
| SSE deadlock after REST idle | Task 3, Task 4 |
| Minimal done JSON (no empty arrays) | Task 1 |
| Snapshot fallback when API messages lag | Task 2, Task 3 |
| Bounded wait (not 1 hour hang) | Task 3 (`STREAM_DRAIN_TIMEOUT`, `RUN_IDLE_TIMEOUT`) |
| Ticket/comment/status apply | Existing `finish_with_apply` — unblocked by Task 3 |
| Drawer refreshes after apply | Task 5 |
| No placeholders | All steps include concrete code |

## Out of scope (follow-ups)

- Auto `in_progress` + `waiting_for_agent` on run start (M05 / separate PR)
- Workflow engine auto-chaining (M05)
- Reducing `RUN_IDLE_TIMEOUT` further based on production metrics

---

## Execution handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-10-opencode-run-completion.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
