# Codex Live Console Activity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore ordered, useful Codex activity in Live Console from Codex CLI 0.145.0 JSONL without exposing the upstream schema to the web client.

**Architecture:** Extend `CodexConsolePublisher` as the sole translation boundary from Codex lifecycle items to Coppice's existing `codex.console.*` events. Lifecycle-capable actions reuse the upstream item ID, allowing the existing reducer to update one card in place; completed reasoning and messages remain append-only entries, and persisted normalized events replay in publication order.

**Tech Stack:** Rust, serde_json, tracing, Tokio broadcast streams, React/TypeScript, Vitest

---

### Task 1: Replace the Codex fixture and establish failing publisher coverage

**Files:**
- Modify: `fixtures/codex/done.jsonl`
- Modify: `server/src/providers/codex_console.rs`

- [ ] **Step 1: Replace the synthetic fixture**

Use sanitized 0.145.0 events with `item.started`, `item.updated`, and `item.completed`, including `reasoning`, `command_execution`, `file_change`, `mcp_tool_call`, and a final `agent_message` contract. Every lifecycle item uses a stable non-empty `item.id` and all typed fields required by the published schema; focused value-based cases cover collaboration, web search, todo, and error items.

- [ ] **Step 2: Write fixture-driven failing tests**

Feed every JSONL line to one publisher and assert an ordered normalized sequence containing:

```rust
assert_eq!(events[1]["type"], "codex.console.thinking");
assert_eq!(events[2]["id"], "cmd_1");
assert_eq!(events[2]["status"], "running");
assert_eq!(events[4]["id"], "cmd_1");
assert_eq!(events[4]["status"], "completed");
assert_eq!(events.last().unwrap()["type"], "codex.console.result");
```

Also assert that file-change output includes all sanitized add/update/delete paths, each supported non-shell type produces a visible tool entry, and only the final agent message becomes a result.

- [ ] **Step 3: Run the publisher test to verify it fails**

Run: `rtk cargo test -p coppice-server providers::codex_console::tests::publishes_sanitized_fixture_in_order`

Expected: FAIL because the current publisher drops reasoning and non-shell activity and omits command IDs.

### Task 2: Normalize the full Codex item lifecycle

**Files:**
- Modify: `server/src/providers/codex_console.rs`

- [ ] **Step 1: Add lifecycle dispatch**

Route `item.started`, `item.updated`, and `item.completed` through an internal lifecycle enum while retaining the existing `thread.started` path. Only completed `agent_message`, `reasoning`, `file_change`, and item-level `error` payloads append entries; command, MCP, collaboration, web-search, and todo-list items publish lifecycle updates.

- [ ] **Step 2: Emit shell updates with stable IDs**

Publish the existing normalized shape with the upstream ID:

```rust
json!({
    "type": "codex.console.tool",
    "id": id,
    "variant": "shell",
    "title": command,
    "status": normalized_status,
    "output": non_empty_aggregated_output,
})
```

Map `in_progress` to `running`, `completed` to `completed`, and `failed`/`declined` to `error`, with lifecycle and exit-code fallbacks for defensive compatibility.

- [ ] **Step 3: Emit concise action updates**

Use `variant: "action"` and the same upstream ID for:

- file changes: title `File changes`, output lines such as `add path/to/file`, status from patch status;
- MCP: title `MCP server.tool`, error message only on failure;
- collaboration: title `Collaboration: spawn agent`, receiver count only;
- web search: title `Web search: query`, lifecycle-derived status;
- todo list: title `To-do list`, `[x]` / `[ ]` lines updated in place;
- item errors: title `Codex error`, status `error`, message output.

Do not serialize MCP arguments/results, collaboration prompts/state messages, web-search action payloads, or any unknown raw object.

- [ ] **Step 4: Handle future/malformed events safely**

Ignore missing IDs and required display fields. Log unknown top-level or item type strings with structured `tracing::debug!` fields only; never interpolate or debug-print the JSON payload.

- [ ] **Step 5: Run targeted Rust tests**

Run: `rtk cargo test -p coppice-server providers::codex_console::tests`

Expected: PASS, including existing text/result behavior and the new lifecycle fixture tests.

### Task 3: Prove artifact replay and reducer correlation

**Files:**
- Modify: `server/src/providers/codex_console.rs`
- Modify: `web/src/features/runs/claude-console-state.test.ts`

- [ ] **Step 1: Test normalized artifact round-trip**

Write the fixture-produced events with `ArtifactService::write_console_events`, read them back, and assert exact vector equality. This proves completed-run replay preserves the normalized publication sequence.

- [ ] **Step 2: Test one-card reducer transitions**

Apply start and completion events with the same ID:

```ts
let state = applyClaudeConsoleEvent(createClaudeConsoleState(), {
  type: 'codex.console.tool', id: 'cmd_1', variant: 'shell',
  status: 'running', title: 'cargo test',
});
state = applyClaudeConsoleEvent(state, {
  type: 'codex.console.tool', id: 'cmd_1', variant: 'shell',
  status: 'completed', title: 'cargo test', output: 'ok',
});
expect(state.entries).toHaveLength(1);
expect(state.entries[0]).toMatchObject({ toolId: 'cmd_1', status: 'completed', output: 'ok' });
```

Also replay thinking, file action, and result events in order and assert the final entry kinds.

- [ ] **Step 3: Run focused web tests**

Run: `rtk npm --prefix web test -- src/features/runs/claude-console-state.test.ts`

Expected: PASS after dependencies are installed through the repository's supported web-test path if needed.

### Task 4: Verify and commit

**Files:**
- Verify all modified files above

- [ ] **Step 1: Format and run focused checks**

Run:

```bash
rtk rustfmt --edition 2021 --check server/src/providers/codex_console.rs
rtk cargo test -p coppice-server providers::codex_console::tests
rtk cargo test -p coppice-server providers::codex::tests
rtk cargo test -p coppice-server workers::job_worker::tests::production_persistence_retains_codex_console_events_in_order -- --exact
rtk cargo test -p coppice-server --features embedded-test-db --test integration_live_console completed_codex_run_replays_structured_fixture_events_in_order -- --exact --test-threads 1
rtk npm --prefix web test -- src/features/runs/claude-console-state.test.ts
rtk cargo clippy --workspace -- -D warnings
```

Expected: all commands PASS without launching `codex exec`.

- [ ] **Step 2: Review scope**

Confirm `git diff` changes only the Codex fixture/publisher, reducer tests, and this plan; result extraction, process cancellation, timeout, connector configuration, and shared renderer implementation remain unchanged.

- [ ] **Step 3: Commit**

```bash
rtk git add docs/superpowers/plans/2026-08-03-codex-live-console.md fixtures/codex/blocked.jsonl fixtures/codex/done.jsonl server/src/providers/codex.rs server/src/providers/codex_console.rs server/src/workers/job_worker.rs server/tests/integration_live_console.rs web/src/features/runs/claude-console-state.test.ts
rtk git commit -m "fix(codex): restore live console activity"
```
