# Context & Long-Running Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add provider-native compaction guard (document + observe), checkpoint `continued` runs with resume context, and PM `splitTickets` with per-status `auto_split` (default pending human approval).

**Architecture:** Two layers — providers compact within-run history; Coppice keeps tickets small via split proposals, checkpoint comments, and a capped `## Resume` section in `context.md`. Split application mirrors M05 `auto_assign` via `auto_split.effective(status)`.

**Tech Stack:** Rust (Axum, SQLx), React/Vite, MockProvider fixtures, OpenCode serve API

**Design spec:** [docs/superpowers/specs/2026-06-10-context-long-running-tasks-design.md](../specs/2026-06-10-context-long-running-tasks-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/010_context_long_running.sql` | `parent_ticket_id`, `pending_split_recommendation` |
| `config/src/lib.rs` | `AutoSplitConfig`, `WorkflowConfig.auto_split` |
| `server/src/domain/workflow.rs` | `PendingSplitRecommendation`, `SplitTicketSpec` |
| `server/src/domain/comment.rs` | Use existing `ProgressUpdate` for `continued` |
| `server/src/providers/mod.rs` | `Continued` variant, `splitTickets` on `Done` |
| `server/src/services/result_contract.rs` | Apply `continued`; extract splits from `done` |
| `server/src/services/run_orchestrator.rs` | Split apply; rename `load_resume_context` → continuation |
| `server/src/services/split_service.rs` | Create children, approve/dismiss pending |
| `server/src/services/context_builder.rs` | PM split rules; engineer `continued` rules |
| `server/src/sessions/opencode_events.rs` | Extract contract from `compaction` parts |
| `server/src/api/tickets.rs` | `approve-splits`, `dismiss-splits`, `children` |
| `fixtures/agent-responses/**/*.json` | `continued.json`, `pm-split-pending.json` |
| `fixtures/opencode-events/compacted-done.jsonl` | Compaction regression fixture |
| `web/src/opencode-session/parts/CompactionPart.tsx` | Live Session compaction UI |
| `web/src/features/tickets/TicketMetadataPanel.tsx` | Pending split card |
| `docs/providers/opencode.md` | Compaction guard section |
| `docs/providers/claude-code.md`, `codex.md` | Native compaction notes |

---

## Phase 1 — Provider guard

### Task 1: OpenCode compaction extraction regression

**Files:**
- Create: `fixtures/opencode-events/compacted-done.jsonl`
- Modify: `server/src/sessions/opencode_events.rs`
- Test: `server/src/sessions/opencode_events.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Add fixture**

`fixtures/opencode-events/compacted-done.jsonl` — assistant message with a `compaction` part followed by final `done` JSON in a `text` part (minimal realistic shape).

- [ ] **Step 2: Write failing test**

```rust
#[test]
fn extract_result_after_compaction_part_in_messages() {
    let messages = vec![serde_json::json!({
        "info": { "role": "assistant" },
        "parts": [
            { "type": "compaction", "text": "Summary of prior work…", "auto": true },
            { "type": "text", "text": r#"{"status":"done","summary":"Done after compact.","nextStatus":"In Review"}"# }
        ]
    })];
    let result = extract_result_from_messages(&messages).expect("extract after compaction");
    match result {
        AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Done after compact."),
        _ => panic!("expected done"),
    }
}
```

- [ ] **Step 3: Run test — expect FAIL**

Run: `cargo test -p coppice-server extract_result_after_compaction_part -- --nocapture`

- [ ] **Step 4: Include `compaction` parts in extraction scan**

In `extract_result_from_messages`, treat `part_type == "compaction"` like `text`/`reasoning` for `extract_result_from_text` (compaction summary may embed contract, but final `text` part is authoritative — scan all part types in reverse order).

- [ ] **Step 5: Run test — expect PASS**

- [ ] **Step 6: Commit**

```bash
git add fixtures/opencode-events/compacted-done.jsonl server/src/sessions/opencode_events.rs
git commit -m "fix(opencode): extract result contract after compaction parts"
```

---

### Task 2: OpenCode compaction docs

**Files:**
- Modify: `docs/providers/opencode.md`
- Modify: `docs/providers/README.md` (one-line pointer)

- [ ] **Step 1: Add "Context compaction" section to opencode.md**

Cover:
- `compaction.auto` default true in `~/.config/opencode/opencode.jsonc`
- Trigger: `token_count >= input_limit - reserved` (reserved default 20k)
- glm-5.1 200K context → compaction rarely fires below ~180K
- Coppice does not call `/compact`; relies on OpenCode guard
- `RUN_IDLE_TIMEOUT` 600s may end run before compaction on marathon tasks — use `continued` checkpoint pattern
- SSE events: `session.compacted`, `session.next.compaction.*`

- [ ] **Step 2: Commit**

```bash
git add docs/providers/opencode.md docs/providers/README.md
git commit -m "docs(opencode): document provider-side context compaction guard"
```

---

### Task 3: Live Session compaction UI

**Files:**
- Create: `web/src/opencode-session/parts/CompactionPart.tsx`
- Modify: `web/src/opencode-session/sync/types.ts` — `CompactionPart` type
- Modify: `web/src/opencode-session/session/AssistantMessage.tsx` — render compaction parts
- Modify: `web/src/opencode-session/sync/reduce-event.ts` — if needed for part types

- [ ] **Step 1: Add `CompactionPart` type**

```typescript
export interface CompactionPart {
  id: string;
  type: 'compaction';
  text: string;
  messageID: string;
  auto?: boolean;
}
```

- [ ] **Step 2: Render collapsed "Context compacted" card**

Match `ReasoningPart` style — label "Context compacted", preview excerpt, expandable markdown body. Show `auto` badge when true.

- [ ] **Step 3: Wire in `AssistantMessage` part switch**

- [ ] **Step 4: Run web tests**

Run: `cd web && yarn test --run`

- [ ] **Step 5: Commit**

```bash
git add web/src/opencode-session/
git commit -m "feat(web): show OpenCode compaction parts in Live Session"
```

---

## Phase 2 — Checkpoint `continued`

### Task 4: `Continued` result contract variant

**Files:**
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/services/result_contract.rs`
- Create: `fixtures/agent-responses/backend_engineer/continued.json`

- [ ] **Step 1: Add `Continued` variant to `AgentRunResult`**

```rust
Continued {
    summary: String,
    #[serde(default, rename = "progressNote")]
    progress_note: Option<String>,
    #[serde(default, rename = "changedFiles")]
    changed_files: Vec<String>,
    #[serde(default, rename = "testsRun")]
    tests_run: Vec<String>,
    #[serde(default)]
    blockers: Vec<String>,
},
```

- [ ] **Step 2: Handle in `apply_agent_result`**

- Run status: `Succeeded`
- Ticket: no status/substatus change
- Comment intent: `CommentIntent::ProgressUpdate` (existing enum — not a new intent)
- Body: `build_done_comment_body` with summary + optional progressNote section

- [ ] **Step 3: Add fixture `fixtures/agent-responses/backend_engineer/continued.json`**

- [ ] **Step 4: Unit test `continued_fixture_maps_to_succeeded_progress`**

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/mod.rs server/src/services/result_contract.rs fixtures/
git commit -m "feat(server): add continued result contract for checkpoint runs"
```

---

### Task 5: Workflow — `continued` does not advance gates

**Files:**
- Modify: `server/src/services/workflow_service.rs`
- Modify: `server/src/services/run_orchestrator.rs`
- Test: `server/src/services/workflow_service.rs`

- [ ] **Step 1: Write failing test**

`continued_run_does_not_change_ticket_status` — `RunOutcome::Succeeded` with `AgentRunResult::Continued` → `resolve_transition` returns `new_status: None`.

- [ ] **Step 2: Branch on `AgentRunResult::Continued` in `resolve_transition`**

Return no-op transition (same as done but without column move). Engineer stays In Progress.

- [ ] **Step 3: `finish_run` handles `Continued` in orchestrator** — same path as done for comment creation; skip workflow status apply when action has `new_status: None`.

- [ ] **Step 4: Commit**

```bash
git add server/src/services/workflow_service.rs server/src/services/run_orchestrator.rs
git commit -m "feat(workflow): continued runs succeed without status transition"
```

---

### Task 6: Extended resume context

**Files:**
- Modify: `server/src/services/run_orchestrator.rs` — rename to `load_run_continuation_context`
- Modify: `server/src/workers/job_worker.rs` — call new loader
- Modify: `server/src/services/context_builder.rs` — engineer checkpoint prompt rules
- Test: `server/src/services/run_orchestrator.rs`

- [ ] **Step 1: Expand loader**

Priority:
1. Existing blocker + clarification answer (M05)
2. Else latest agent comment with `intent == ProgressUpdate` on ticket
3. Cap combined `## Resume` section at 2000 chars (`truncate_with_ellipsis`)

Format:
```markdown
## Resume

**Last checkpoint:** {body}

**Prior blocker:** {blocker} / **PM answer:** {answer}  (only if applicable)
```

- [ ] **Step 2: Add context_builder rules for non-PM agents**

```text
## Coppice platform rules — long tasks (required)
- Prefer `status: "continued"` with `progressNote` when substantial work remains and the session is getting long.
- Use `status: "done"` only when acceptance criteria are met.
- Use `status: "blocked"` when genuinely stuck.
```

- [ ] **Step 3: Unit test `continuation_context_includes_progress_update`**

- [ ] **Step 4: MockProvider integration** — `backend_engineer/continued.json` then second run reads resume section in written context file (extend existing mock test pattern).

- [ ] **Step 5: Commit**

```bash
git add server/src/services/run_orchestrator.rs server/src/workers/job_worker.rs server/src/services/context_builder.rs
git commit -m "feat(server): resume context from continued checkpoint comments"
```

---

## Phase 3 — PM split (pending default)

### Task 7: Database migration

**Files:**
- Create: `server/migrations/010_context_long_running.sql`
- Modify: `server/src/domain/ticket.rs`
- Modify: `server/src/services/ticket_service.rs`

- [ ] **Step 1: Migration**

```sql
ALTER TABLE tickets
    ADD COLUMN parent_ticket_id UUID REFERENCES tickets(id) ON DELETE SET NULL,
    ADD COLUMN pending_split_recommendation JSONB;

CREATE INDEX tickets_parent_ticket_id_idx ON tickets(parent_ticket_id);
```

- [ ] **Step 2: Run migrate**

Run: `cargo run -p coppice-cli -- migrate`

- [ ] **Step 3: Commit**

```bash
git add server/migrations/010_context_long_running.sql
git commit -m "feat(server): add parent ticket and pending split columns"
```

---

### Task 8: `auto_split` config

**Files:**
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`
- Modify: `config.toml` (local example only if tracked — use config.example.toml)

- [ ] **Step 1: Add `AutoSplitConfig`** — copy `AutoAssignConfig` structure; `WorkflowConfig.auto_split`; `Default` with `default: false`.

- [ ] **Step 2: Test `auto_split_default_false`**

- [ ] **Step 3: Add to config.example.toml**

```toml
[workflow.auto_split]
default = false
```

- [ ] **Step 4: Commit**

```bash
git add config/src/lib.rs config.example.toml
git commit -m "feat(config): add workflow.auto_split per-status gate"
```

---

### Task 9: Split domain types + contract fields

**Files:**
- Modify: `server/src/domain/workflow.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/services/context_builder.rs` — PM split platform rules

- [ ] **Step 1: Add types**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitTicketSpec {
    pub title: String,
    pub description: String,
    #[serde(default, rename = "acceptanceCriteria")]
    pub acceptance_criteria: Option<String>,
    #[serde(default, rename = "assignTo")]
    pub assign_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSplitRecommendation {
    pub recommended_by_agent_id: Uuid,
    pub recommended_at: String,
    pub splits: Vec<SplitTicketSpec>,
}
```

- [ ] **Step 2: Add to `AgentRunResult::Done`**

```rust
#[serde(default, rename = "splitTickets")]
split_tickets: Vec<SplitTicketSpec>,
```

- [ ] **Step 3: PM platform rules in `format_contract_guidance`**

Add split vs enrich rules from spec (Coppice-owned, not soul).

- [ ] **Step 4: Commit**

```bash
git add server/src/domain/workflow.rs server/src/providers/mod.rs server/src/services/context_builder.rs
git commit -m "feat(server): splitTickets on done contract and PM platform rules"
```

---

### Task 10: SplitService + orchestrator apply (pending path)

**Files:**
- Create: `server/src/services/split_service.rs`
- Modify: `server/src/services/mod.rs`
- Modify: `server/src/services/run_orchestrator.rs`
- Create: `fixtures/agent-responses/pm/split_pending.json`

- [ ] **Step 1: `SplitService::apply_splits`**

```rust
pub async fn apply_splits(
    &self,
    parent: &Ticket,
    splits: &[SplitTicketSpec],
    recommended_by: Uuid,
    auto_split: bool,
) -> Result<ApplySplitOutcome, SplitError>
```

- `auto_split == false` → set `pending_split_recommendation` JSON on parent; return `Pending`
- `auto_split == true` → delegate to `create_child_tickets` (Task 13)

- [ ] **Step 2: `create_child_tickets` (shared helper, used by approve + auto)**

For each spec:
- `TicketService::create` in same `project_id`, `parent_ticket_id = parent.id`
- Merge description + acceptance criteria via `merge_ticket_description`
- Apply `assignTo` on child using `auto_assign.effective(child.status)`

- [ ] **Step 3: Wire in `finish_run` after description merge**

Read `auto_split.effective(current_status)` from workflow config; if `split_tickets` non-empty, call `SplitService`.

- [ ] **Step 4: Unit test with `auto_split` false** — pending JSON set, zero children in DB.

- [ ] **Step 5: Commit**

```bash
git add server/src/services/split_service.rs server/src/services/run_orchestrator.rs fixtures/
git commit -m "feat(server): pending split recommendation when auto_split is false"
```

---

### Task 11: Approve / dismiss splits API

**Files:**
- Modify: `server/src/api/tickets.rs`
- Modify: `server/src/services/split_service.rs`
- Test: `server/tests/integration_tickets.rs` or new `integration_splits.rs`

- [ ] **Step 1: Endpoints**

```text
POST /api/tickets/:id/approve-splits  → create_child_tickets from pending; clear pending
POST /api/tickets/:id/dismiss-splits  → clear pending only
GET  /api/tickets/:id/children        → list tickets where parent_ticket_id = :id
```

- [ ] **Step 2: Integration test `approve_splits_creates_children`**

- [ ] **Step 3: Commit**

```bash
git add server/src/api/tickets.rs server/src/services/split_service.rs server/tests/
git commit -m "feat(api): approve and dismiss pending ticket splits"
```

---

### Task 12: Frontend pending split UI

**Files:**
- Modify: `web/src/lib/schemas/ticket.ts`
- Modify: `web/src/features/tickets/TicketMetadataPanel.tsx`
- Modify: `web/src/features/tickets/useTicket.ts` — approve/dismiss mutations
- Modify: `web/src/features/tickets/TicketDetailPanel.tsx` — optional children list

- [ ] **Step 1: Schema fields**

`parentTicketId`, `pendingSplitRecommendation: { recommendedByAgentId, recommendedAt, splits[] }`

- [ ] **Step 2: Metadata panel card**

When `pendingSplitRecommendation` present:
- List child titles (preview)
- **Approve splits** button → confirm dialog → `POST approve-splits`
- **Dismiss** → `POST dismiss-splits`

- [ ] **Step 3: Optional children section in detail tab**

Link to child tickets if `GET children` returns any.

- [ ] **Step 4: Run `yarn test`**

- [ ] **Step 5: Commit**

```bash
git add web/src/
git commit -m "feat(web): pending split recommendation approve/dismiss UI"
```

---

## Phase 4 — Auto-split + provider docs

### Task 13: Auto-create children when `auto_split` true

**Files:**
- Modify: `server/src/services/split_service.rs`
- Modify: `deploy/config/default.toml` — document only, no default true
- Test: `server/tests/integration_workflow.rs` or dedicated split test

- [ ] **Step 1: Integration test with config override**

```toml
[workflow.auto_split]
default = true
```

PM fixture `pm/split_auto.json` → children created immediately with `parent_ticket_id`.

- [ ] **Step 2: Verify child `assignTo` respects `auto_assign` on child status (Backlog)**

- [ ] **Step 3: Commit**

```bash
git add server/src/services/split_service.rs server/tests/ fixtures/
git commit -m "feat(server): auto-create child tickets when auto_split enabled"
```

---

### Task 14: Provider docs (Claude Code, Codex)

**Files:**
- Modify: `docs/providers/claude-code.md`
- Modify: `docs/providers/codex.md`

- [ ] **Step 1: Add "Context compaction" subsection**

State: compaction is provider-native; Coppice does not reimplement; use `continued` for cross-run checkpoints; refer to opencode.md for serve-mode details.

- [ ] **Step 2: Commit**

```bash
git add docs/providers/claude-code.md docs/providers/codex.md
git commit -m "docs(providers): note native compaction guard for claude-code and codex"
```

---

### Task 15: Acceptance + CI smoke

**Files:**
- Create: `e2e/smoke/m06-context.mjs` (or extend m05)
- Modify: `Makefile` — `e2e-smoke-m06` target
- Modify: `docs/development.md` — brief note on `continued` and splits
- Modify: `AGENTS.md` — pointer to spec

- [ ] **Step 1: Integration test bundle**

Run: `cargo test --workspace` and `make web-test`

Add `scope_continued_run_keeps_in_progress` and `scope_pm_split_pending` to workflow integration file.

- [ ] **Step 2: Optional E2E** — PM run with mock, pending split badge visible (stretch).

- [ ] **Step 3: Run `make clean` after full test pass** (per AGENTS.md disk guidance).

- [ ] **Step 4: Commit**

```bash
git add server/tests/ e2e/ Makefile docs/ AGENTS.md
git commit -m "test: context long-running acceptance and docs"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| OpenCode compaction docs | 2 |
| Compaction UI + extraction | 1, 3 |
| `continued` contract | 4, 5 |
| Resume context | 6 |
| `splitTickets` contract | 9 |
| `auto_split` default false | 8, 10 |
| Approve-splits API | 11 |
| Frontend pending UI | 12 |
| Auto-split true path | 13 |
| Claude/Codex docs | 14 |
| PM rules in context_builder | 6, 9 |
| Acceptance tests | 15 |

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-10-context-long-running-tasks.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks
2. **Inline Execution** — implement tasks in this session with checkpoints

Which approach do you want?
