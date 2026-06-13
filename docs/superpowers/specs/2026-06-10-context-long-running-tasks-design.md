# Context & Long-Running Tasks — Design

**Status:** Approved  
**Date:** 2026-06-10  
**Depends on:** M03 (agent runs), M04 (OpenCode live session), M05 (workflow)  
**Related:** M06 (context budget — complementary, not replaced)

## Problem

Agent runs can exceed practical context limits. Today:

- Coppice writes a fresh `.agent/context.md` per run with full ticket description; no token budget.
- `load_resume_context` only covers blocker → PM clarification, not general progress.
- PM can only **enrich** a ticket (`updatedDescription`); splitting is mentioned in soul but not in the contract or API.
- OpenCode auto-compaction exists but never fired on glm-5.1 runs (~50k tokens vs ~180k threshold); Coppice does not observe compaction events.
- `RUN_IDLE_TIMEOUT` (10 min) can end a run before provider compaction matters on some models.

We need **two layers**: provider-native compaction as guard, Coppice mechanisms to keep tickets small and support multi-run continuity.

## Goals

1. **Provider guard** — rely on OpenCode / Claude Code / Codex within-run compaction; document and verify; surface in Live Session when it happens.
2. **Small tickets** — PM can **split** oversized work into child tickets, not only enrich one card.
3. **Checkpoint runs** — agents can stop after a long turn, post progress, and continue on the next run.
4. **Cross-run resume** — inject compact progress into `context.md` without duplicating full ticket history.
5. **Human gates by default** — split proposals require approval unless `auto_split` is enabled for that status.

## Non-goals (this spec)

- Coppice-owned within-run summarization (providers already do this).
- Full M06 knowledge retrieval / pgvector (separate milestone; `previous_attempt_summary` here is a thin placeholder).
- YAML workflow rules — gates stay in `WorkflowService` (Rust).
- Automatic parent-ticket epic management beyond link + optional parent close recommendation.

---

## Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│ Within-run guard (provider-owned)                            │
│  OpenCode compaction.auto │ Claude Code │ Codex              │
│  → summarizes tool-heavy session history in same session     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Coppice cross-run (ticket + run orchestration)               │
│  1. PM splitTickets[] → child tickets (pending or auto)      │
│  2. status: continued → checkpoint comment + next run        │
│  3. context.md ## Resume ← last checkpoint / blocker       │
└─────────────────────────────────────────────────────────────┘
```

**Principle:** Providers minimize loss when a single run grows unexpectedly. Coppice minimizes how much enters the next run by keeping tickets small and checkpoints explicit.

---

## Layer 1 — Provider guard

### OpenCode (implemented connector)

- Auto-compaction: `compaction.auto` default `true` in OpenCode config; trigger ≈ `input_limit - reserved` (reserved default 20k).
- Coppice uses `opencode serve` HTTP API — same server as TUI; compaction applies.
- Coppice changes:
  - Document thresholds and `~/.config/opencode/opencode.jsonc` knobs in `docs/providers/opencode.md`.
  - Handle SSE `session.compacted` / `session.next.compaction.*` in Live Session (optional `CompactionPart` UI).
  - Verify result contract extraction still works after compaction (integration test with mocked compacted snapshot).
  - Do **not** call `/compact` proactively from Coppice unless manual operator action added later.

### Claude Code / Codex (connectors)

- Document each provider's native compaction behavior in `docs/providers/`.
- Coppice does not implement a parallel summarizer; `TmuxStream` / terminal parsers rely on provider output unchanged.

### Coppice idle timeout

- Keep `RUN_IDLE_TIMEOUT` (600s) as run-level safety; document that very long single runs may need timeout increase or checkpoint `continued` pattern instead of one marathon session.
- Future: configurable `agent.connectors.opencode.run_idle_timeout_secs` (out of scope unless needed during impl).

---

## Layer 2 — Coppice cross-run

### 2a PM ticket split (`splitTickets`)

PM may return child ticket specs instead of (or in addition to) enriching the parent.

**Result contract (PM `done`):**

```json
{
  "status": "done",
  "summary": "Proposed splitting into 3 implementation tickets.",
  "splitTickets": [
    {
      "title": "Implement TmuxStream",
      "description": "## Scope\n…",
      "acceptanceCriteria": "- [ ] …",
      "assignTo": "backend_engineer"
    }
  ],
  "updatedDescription": "Optional parent/epic note (short)",
  "assignTo": null
}
```

**Rules (injected via `context_builder` for PM agents — Coppice-owned, not soul):**

- Use `splitTickets` when work has **multiple independent deliverables** or description would exceed ~2–3 screens.
- Each child must be **self-contained** (title, description, acceptanceCriteria).
- Parent `updatedDescription` should be a **short epic summary**, not a copy of all children.
- Do not set both a huge `updatedDescription` and `splitTickets` with duplicate content.

**Application (`auto_split` gate — mirrors `auto_assign`):**

| `auto_split.effective(status)` | Behavior |
|------------------------------|----------|
| `false` (default) | Store `splitTickets` as **pending split recommendation** on parent ticket; human approves via UI |
| `true` | Create child tickets immediately in same project; link to parent |

**Default config (pending = B):**

```toml
[workflow]
auto_start_runs = false

[workflow.auto_assign]
default = true
backlog = false

[workflow.auto_split]
default = false          # pending recommendation everywhere unless overridden
# backlog = false        # explicit; same as default
# ready = true           # optional: auto-create splits when PM runs at Ready
```

Resolution: `auto_split.effective(status)` — same pattern as `AutoAssignConfig.effective()`.

**Schema:**

```sql
-- tickets
parent_ticket_id UUID NULL REFERENCES tickets(id),
pending_split_recommendation JSONB NULL,
-- shape: { recommendedByAgentId, recommendedAt, splits: SplitTicketSpec[] }

-- optional index for children-of-parent queries
CREATE INDEX tickets_parent_ticket_id_idx ON tickets(parent_ticket_id);
```

**`SplitTicketSpec` (domain + API):**

```rust
struct SplitTicketSpec {
    title: String,
    description: String,
    acceptance_criteria: Option<String>,
    assign_to: Option<String>,  // agent key; uses same auto_assign gate on child creation
}
```

**Workflow on PM run with splits:**

1. Apply parent `updatedDescription` / AC if present (short epic only).
2. If `splitTickets` non-empty:
   - `auto_split` false → set `pending_split_recommendation`; comment summarizes proposed splits.
   - `auto_split` true → `TicketService::create_children(...)`, set `parent_ticket_id`, apply per-child `assignTo` per child ticket's status + `auto_assign`.
3. Parent status: default **stay** at current column unless workflow gate says otherwise (e.g. Backlog → Ready after PM succeeded). Parent does **not** auto-close.
4. Human can **Approve splits** → creates children, clears pending. **Dismiss** → clears pending.

**API:**

```text
POST /api/tickets/:id/approve-splits   # creates children from pending
POST /api/tickets/:id/dismiss-splits   # clears pending
GET  /api/tickets/:id/children         # list child tickets (optional v1: include in ticket detail)
```

**UI:**

- Metadata panel: pending split recommendation card (like assign recommendation badge).
- Approve opens preview list of child titles before confirm.
- Board: optional visual hint for child tickets (parent link in drawer).

---

### 2b Checkpoint runs (`status: "continued"`)

For long **implementation** runs (engineer, etc.), agent stops deliberately before context pressure.

**Result contract:**

```json
{
  "status": "continued",
  "summary": "Implemented TmuxStream create/kill; capture loop next.",
  "progressNote": "Files touched: server/src/sessions/tmux_stream.rs. Tests not yet run.",
  "changedFiles": ["server/src/sessions/tmux_stream.rs"],
  "testsRun": [],
  "blockers": []
}
```

**Server behavior:**

| Field | Action |
|-------|--------|
| `summary` | Agent comment (intent: `implementation_progress` — new comment intent) |
| `progressNote` | Appended to comment or stored as run `checkpoint_summary` on `agent_runs` |
| `changedFiles`, `testsRun` | Comment lists (same as `done`) |
| Run status | **`succeeded`** (not failed) — intentional pause |
| Ticket status | **Unchanged** — still In Progress |
| Next run | **Not** auto-started by default; human clicks Run or `auto_start_runs` + explicit `continued` auto-continue config (future; v1 = manual Run) |

**Prompt rules (context_builder, all non-PM agents):**

- If approaching turn budget (~many tool rounds) or scope is multi-session, return `continued` with clear `progressNote` instead of rushing a partial `done`.
- Do not use `continued` to avoid writing tests — use `blocked` if genuinely stuck.

**Turn budget signal:**

- v1: **prompt-only** (“prefer `continued` after substantial progress if work remains”).
- v2 (optional): server passes `run_continuation_count` in context from prior `continued` runs on same ticket+agent.

---

### 2c Extended resume context

Expand `load_resume_context` → `load_run_continuation_context`:

| Source | When included |
|--------|----------------|
| Blocker + clarification answer | Existing M05 path |
| Last `continued` run comment | Most recent `implementation_progress` intent |
| Last succeeded run summary | If ticket still In Progress and same assignee |

Injected as:

```markdown
## Resume

**Last checkpoint:** …
**Prior blocker:** … (if any)
```

Cap section at **~2k characters** (Coppice-side truncation with ellipsis); full text remains in comments.

---

## Configuration summary

```toml
[workflow.auto_split]
default = false
# Per-status overrides: backlog, ready, in_progress, …
```

Independent of `auto_assign` and `auto_start_runs`.

---

## Testing strategy

| Area | Tests |
|------|-------|
| `auto_split` false | PM fixture with `splitTickets` → pending JSON, no children |
| `auto_split` true | Children created with `parent_ticket_id` |
| `approve-splits` API | Creates children, clears pending |
| `continued` contract | Run succeeded, ticket status unchanged, comment intent |
| Resume context | Continued comment appears in `context.md` |
| OpenCode compaction | Fixture snapshot with `CompactionPart` still extracts `done` JSON |
| Mock provider | `continued.json`, `pm-split-pending.json` fixtures |

---

## Implementation phases

**Phase 1 — Provider guard (low risk)**  
Docs + Live Session compaction events + extraction regression test.

**Phase 2 — Checkpoint `continued`**  
Contract variant, comment intent, resume context, context_builder prompts.

**Phase 3 — PM split (pending default)**  
Schema, `auto_split` config, pending recommendation, approve/dismiss API + UI.

**Phase 4 — `auto_split` true path + child ticket board UX**  
Auto-create children when config enabled per status.

M06 context budget can later replace the 2k resume cap with structured sections.

---

## Acceptance criteria

- [ ] OpenCode compaction documented; Coppice does not duplicate within-run summarization.
- [ ] `continued` ends run as succeeded with progress comment; next run includes resume section.
- [ ] PM can return `splitTickets`; default **pending** until human approves.
- [ ] `[workflow.auto_split]` mirrors `[workflow.auto_assign]` shape; `default = false`.
- [ ] Approve-splits creates linked child tickets in same project.
- [ ] PM platform rules in `context_builder` cover split vs enrich (not agent soul).
- [ ] Provider connectors (Claude Code, Codex) documented as relying on native compaction.

---

## Open questions (deferred)

- Auto-continue after `continued` when `auto_start_runs = true` — defer to Phase 2.5 if needed in testing.
- Parent ticket auto-close when all children Done — human gate for v1.
- `run_idle_timeout` per connector config — add if 10 min bites during Claude Code testing.
