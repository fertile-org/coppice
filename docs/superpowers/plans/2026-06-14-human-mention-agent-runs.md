# Human @Mention Agent Runs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let humans `@mention` an agent in a ticket comment and start an immediate run — **Agent** mode (execute in shared worktree via `work_on_ticket`) or **Chat** mode (reply in thread via `respond_to_mention`) — with human-first context profiles and no workflow side-effects on human Agent runs.

**Architecture:** Add `context_profile` + `trigger_comment_id` to `agent_runs`. Extend `ContextInput` with profile-specific assembly (human request block first; omit heavy thread for `human_agent`). Comments API accepts `mentionMode`, validates one mention, always enqueues runs (independent of `auto_start_runs`). Orchestrator skips gate transitions when profile is `human_agent`.

**Tech Stack:** Rust (Axum, SQLx), React/Vite/TanStack Query

**Design spec:** [docs/superpowers/specs/2026-06-14-human-mention-agent-runs-design.md](../specs/2026-06-14-human-mention-agent-runs-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/011_human_mention_runs.sql` | `context_profile`, `trigger_comment_id` on `agent_runs` |
| `server/src/domain/context_profile.rs` | `ContextProfile` enum + str conversion |
| `server/src/domain/run.rs` | `AgentRun.context_profile`, `trigger_comment_id` |
| `server/src/services/run_service.rs` | `StartRunOptions`, INSERT/SELECT/row_to_run |
| `server/src/services/context_builder.rs` | Profile branches, human request block, on-demand JSON |
| `server/src/services/run_orchestrator.rs` | Skip workflow when `human_agent`; load thread only for `full` |
| `server/src/services/workflow_service.rs` | Skip run-start transition for `human_agent` |
| `server/src/workers/job_worker.rs` | Load profile + trigger comment; write `.agent/*.json` |
| `server/src/api/comments.rs` | `mentionMode`, validation, `startedRuns` response |
| `web/src/lib/schemas/ticket.ts` | `mentionMode` on create comment schema |
| `web/src/features/tickets/useTicket.ts` | Pass `mentionMode`; handle `startedRuns` |
| `web/src/features/tickets/TicketCommentsTab.tsx` | Mode `<select>`, @ autocomplete, toast |

---

### Task 1: Database migration + domain types

**Files:**
- Create: `server/migrations/011_human_mention_runs.sql`
- Create: `server/src/domain/context_profile.rs`
- Modify: `server/src/domain/mod.rs`
- Modify: `server/src/domain/run.rs`

- [ ] **Step 1: Write migration**

```sql
ALTER TABLE agent_runs
  ADD COLUMN context_profile TEXT NOT NULL DEFAULT 'full',
  ADD COLUMN trigger_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL;

ALTER TABLE agent_runs
  ADD CONSTRAINT agent_runs_context_profile_check
  CHECK (context_profile IN ('full', 'human_agent', 'human_chat'));
```

- [ ] **Step 2: Add `ContextProfile` enum**

Create `server/src/domain/context_profile.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextProfile {
    Full,
    HumanAgent,
    HumanChat,
}

impl ContextProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::HumanAgent => "human_agent",
            Self::HumanChat => "human_chat",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "human_agent" => Some(Self::HumanAgent),
            "human_chat" => Some(Self::HumanChat),
            _ => None,
        }
    }
}
```

Re-export from `domain/mod.rs`.

- [ ] **Step 3: Extend `AgentRun`**

Add to `server/src/domain/run.rs`:

```rust
pub context_profile: ContextProfile,
pub trigger_comment_id: Option<Uuid>,
```

Default new runs to `ContextProfile::Full` in `row_to_run` when column missing (tests only — migration handles prod).

- [ ] **Step 4: Run migration**

Run: `cargo run -p coppice-cli -- migrate`

Expected: migration `011_human_mention_runs` applied

- [ ] **Step 5: Commit**

```bash
git add server/migrations/011_human_mention_runs.sql server/src/domain/
git commit -m "$(cat <<'EOF'
Add context_profile and trigger_comment_id to agent_runs.

Supports human @mention runs with profile-specific context assembly.
EOF
)"
```

---

### Task 2: RunService — start runs with profile metadata

**Files:**
- Modify: `server/src/services/run_service.rs`

- [ ] **Step 1: Add `StartRunOptions`**

```rust
pub struct StartRunOptions {
    pub context_profile: ContextProfile,
    pub trigger_comment_id: Option<Uuid>,
}

impl Default for StartRunOptions {
    fn default() -> Self {
        Self {
            context_profile: ContextProfile::Full,
            trigger_comment_id: None,
        }
    }
}
```

- [ ] **Step 2: Extend `start_run_for_agent` signature**

Change from:

```rust
pub async fn start_run_for_agent(
    &self,
    ticket_id: Uuid,
    agent_id: Uuid,
    job_type: &str,
) -> Result<AgentRun, RunError>
```

To:

```rust
pub async fn start_run_for_agent(
    &self,
    ticket_id: Uuid,
    agent_id: Uuid,
    job_type: &str,
    options: StartRunOptions,
) -> Result<AgentRun, RunError>
```

Update INSERT to include `context_profile`, `trigger_comment_id`. Update all `RETURNING` / `SELECT` lists and `row_to_run` to read both columns.

- [ ] **Step 3: Update all call sites**

Pass `StartRunOptions::default()` from:
- `run_orchestrator.rs` (follow-up runs, resume)
- Any ticket/run API handlers that call `start_run_for_agent`

Human mention path (Task 6) passes explicit profile + `trigger_comment_id`.

- [ ] **Step 4: Unit test roundtrip**

Add test in `run.rs` or `context_profile.rs`:

```rust
#[test]
fn context_profile_roundtrip() {
    for profile in [ContextProfile::Full, ContextProfile::HumanAgent, ContextProfile::HumanChat] {
        assert_eq!(ContextProfile::from_str(profile.as_str()), Some(profile));
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add server/src/services/run_service.rs server/src/domain/run.rs
git commit -m "$(cat <<'EOF'
Store context profile and trigger comment when starting agent runs.
EOF
)"
```

---

### Task 3: Context builder — profiles + on-demand JSON

**Files:**
- Modify: `server/src/services/context_builder.rs`

- [ ] **Step 1: Extend `ContextInput`**

Add fields:

```rust
pub context_profile: ContextProfile,
pub human_request: Option<HumanRequest<'a>>,  // body, posted_at, mode label
pub ticket_id: Option<Uuid>,
pub assignee_agent_key: Option<&'a str>,
pub thread_excerpt: Option<&'a str>,  // human_chat only; pre-formatted markdown
```

```rust
pub struct HumanRequest<'a> {
    pub body: &'a str,
    pub posted_at: &'a str,
    pub mode_label: &'a str,  // "Agent" | "Chat"
}
```

- [ ] **Step 2: Branch `build_context_md` on profile**

| Profile | Behavior |
|---------|----------|
| `Full` | Current template unchanged (description + resume/thread section) |
| `HumanAgent` | Human request block first → ticket snapshot (title, status, substatus, assignee, id) → agent role → repository → scoped platform rules (git + verification; omit PM refinement, omit assignTo in contract guidance) → contract |
| `HumanChat` | Human request block → short thread excerpt → minimal ticket snapshot (title, status) → agent role → chat-oriented contract (concise reply; no worktree execution emphasis) |

Human request block (spec):

```markdown
# Human request (read this first)

**From:** Human
**Posted:** <ISO8601>
**Mode:** Agent | Chat

> <full comment body>

This instruction overrides ticket description and thread summaries when they conflict.
```

For `HumanAgent`, append:

```markdown
Execute in the ticket worktree unless the request is purely informational (then reply in your result summary only).
```

Add on-demand section for `HumanAgent` + `HumanChat`:

```markdown
## On-demand ticket data

If you need full description, history, or past runs, read:
- `.agent/ticket.json`
- `.agent/comments.json`
- `.agent/runs.json`

Do not load these unless necessary for the human request.
```

- [ ] **Step 3: Add JSON snapshot writers**

```rust
pub fn write_agent_context_files(
    worktree: &Path,
    ticket_json: &serde_json::Value,
    comments_json: &serde_json::Value,
    runs_json: &serde_json::Value,
) -> std::io::Result<()>;

pub fn write_context_file(worktree: &Path, input: &ContextInput) -> std::io::Result<()>;
// write_context_file calls build_context_md + write_agent_context_files when profile != Full
```

Keep `.agent/` gitignored (existing behavior).

- [ ] **Step 4: Unit tests**

Add tests:

```rust
#[test]
fn human_agent_omits_description_and_full_thread() { /* ... */ }

#[test]
fn human_agent_puts_human_request_first() { /* ... */ }

#[test]
fn human_chat_includes_short_excerpt_only() { /* ... */ }

#[test]
fn full_profile_unchanged() { /* assert existing assertions still pass */ }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p coppice-server context_builder -- --nocapture`

Expected: all pass

- [ ] **Step 6: Commit**

```bash
git add server/src/services/context_builder.rs
git commit -m "$(cat <<'EOF'
Add human mention context profiles and on-demand JSON snapshots.
EOF
)"
```

---

### Task 4: Job worker — load profile and assemble context

**Files:**
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/src/services/run_orchestrator.rs` (`load_run_continuation_context`)

- [ ] **Step 1: Gate thread loading by profile**

In `load_run_continuation_context`, accept `context_profile`:

- `Full` → existing `format_ticket_thread` (up to 4000 chars)
- `HumanAgent` → `None` (no inline thread)
- `HumanChat` → new helper `format_thread_excerpt(comments, agent_names, max_comments: 3, max_chars: 800)`

- [ ] **Step 2: Load trigger comment in job worker**

When `run.trigger_comment_id` is set:

```rust
let trigger = CommentService::new(pool).get(trigger_comment_id).await?;
// build HumanRequest from trigger.body + trigger.created_at + mode from profile
```

- [ ] **Step 3: Build `ContextInput` with profile fields**

For `human_agent`:
- `ticket_description`: empty or one-line snapshot (not full description)
- `resume_context`: None
- Resolve assignee agent key for snapshot

For `human_chat`:
- `thread_excerpt`: from excerpt helper
- Minimal description

- [ ] **Step 4: Write JSON snapshots before provider call**

Query ticket, all comments (newest first), last 10 runs for ticket. Serialize to JSON. Call `write_agent_context_files` + `write_context_file`.

- [ ] **Step 5: Skip run-start workflow for `human_agent`**

In `execute_job`, before `resolve_run_start_transition`:

```rust
if run.context_profile != ContextProfile::HumanAgent {
    if let Some(new_status) = WorkflowService::resolve_run_start_transition(...) { ... }
}
```

- [ ] **Step 6: Commit**

```bash
git add server/src/workers/job_worker.rs server/src/services/run_orchestrator.rs
git commit -m "$(cat <<'EOF'
Build human mention context in job worker and skip run-start gates.
EOF
)"
```

---

### Task 5: Orchestrator — skip post-run workflow for human_agent

**Files:**
- Modify: `server/src/services/run_orchestrator.rs`
- Modify: `server/src/domain/workflow.rs`

- [ ] **Step 1: Add `context_profile` to `TransitionContext`**

```rust
pub context_profile: ContextProfile,
```

Populate from `run.context_profile` in `finish_run`.

- [ ] **Step 2: Short-circuit workflow for human_agent success**

After building `TransitionContext`, before `resolve_transition`:

```rust
let action = if ctx.context_profile == ContextProfile::HumanAgent && ctx.run_outcome == RunOutcome::Succeeded {
    TransitionAction::noop()  // or equivalent: new_status = None, no assignee change
} else {
    WorkflowService::resolve_transition(ctx)?
};
```

Ensure agent comment + git footer still post on success (existing path after transition).

Blocked/failed paths unchanged.

- [ ] **Step 3: Unit test**

In `run_orchestrator` or `workflow_service` tests:

```rust
#[test]
fn human_agent_done_does_not_change_status() {
    // TransitionContext with HumanAgent + Succeeded → action.new_status is None
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server run_orchestrator workflow_service -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add server/src/services/run_orchestrator.rs server/src/domain/workflow.rs
git commit -m "$(cat <<'EOF'
Skip workflow gate transitions after human @mention Agent runs complete.
EOF
)"
```

---

### Task 6: Comments API — mentionMode and run enqueue

**Files:**
- Modify: `server/src/api/comments.rs`

- [ ] **Step 1: Extend request/response types**

```rust
#[derive(Deserialize)]
struct CreateCommentBody {
    // existing fields...
    mention_mode: Option<String>,  // "agent" | "chat", default "agent"
}

#[derive(Serialize)]
struct StartedRunSummary {
    run_id: Uuid,
    agent_id: Uuid,
    agent_key: String,
}

#[derive(Serialize)]
struct CreateCommentResponse {
    #[serde(flatten)]
    comment: CommentResponse,
    started_runs: Vec<StartedRunSummary>,
}
```

Change handler return type to `CreateCommentResponse`.

- [ ] **Step 2: Parse mention mode**

```rust
fn parse_mention_mode(raw: Option<&str>) -> Result<MentionMode, CommentError> {
    match raw.unwrap_or("agent") {
        "agent" => Ok(MentionMode::Agent),
        "chat" => Ok(MentionMode::Chat),
        other => Err(CommentError::Validation(format!("invalid mentionMode: {other}"))),
    }
}
```

- [ ] **Step 3: Validation after mention parse**

```rust
if parsed_mentions.is_empty() {
    // return comment only, started_runs: []
}
if parsed_mentions.len() > 1 {
    return Err(CommentError::Validation(
        "only one @mention per comment is supported".into(),
    ));
}
if mention_mode == MentionMode::Agent && ticket.ticket.repo_id.is_none() {
    return Err(CommentError::Validation(
        "repository required to run agent in worktree".into(),
    ));
}
```

- [ ] **Step 4: Start run with profile (always, not gated on auto_start_runs)**

Replace current block that checks `state.config.workflow.auto_start_runs`:

```rust
let (job_type, profile) = match mention_mode {
    MentionMode::Agent => ("work_on_ticket", ContextProfile::HumanAgent),
    MentionMode::Chat => ("respond_to_mention", ContextProfile::HumanChat),
};

let run = run_svc
    .start_run_for_agent(
        ticket_id,
        mention.mentioned_agent_id,
        job_type,
        StartRunOptions {
            context_profile: profile,
            trigger_comment_id: Some(comment.id),
        },
    )
    .await
    .map_err(map_run_error)?;
```

Handle `ActiveRunExists` gracefully (return 409 or include in error message — match existing Run Agent behavior).

- [ ] **Step 5: Integration test**

Add test in `server/tests/` or inline module:

1. Post comment with `@pm` + `mentionMode=chat` → run job_type `respond_to_mention`, profile `human_chat`
2. Post comment with `@backend_engineer` + `mentionMode=agent` → `work_on_ticket`, profile `human_agent`
3. Two mentions → 400

- [ ] **Step 6: Commit**

```bash
git add server/src/api/comments.rs server/tests/
git commit -m "$(cat <<'EOF'
Start human @mention runs from comments with Agent/Chat modes.
EOF
)"
```

---

### Task 7: Web — comment composer UX

**Files:**
- Modify: `web/src/lib/schemas/ticket.ts`
- Modify: `web/src/features/tickets/useTicket.ts`
- Modify: `web/src/features/tickets/TicketCommentsTab.tsx`

- [ ] **Step 1: Schema**

```typescript
export const mentionModeSchema = z.enum(['agent', 'chat']);
export type MentionMode = z.infer<typeof mentionModeSchema>;

export const createCommentSchema = z.object({
  body: z.string().min(1),
  mentionMode: mentionModeSchema.optional(),
  attachmentIds: z.array(z.string().uuid()).optional(),
});
```

- [ ] **Step 2: Response type + `postComment`**

```typescript
export interface StartedRunSummary {
  runId: string;
  agentId: string;
  agentKey: string;
}

export interface CreateCommentResponse extends Comment {
  startedRuns?: StartedRunSummary[];
}
```

In `useCreateComment` `onSuccess`:
- Prepend comment as today
- If `startedRuns?.length`, invalidate runs query + show toast ("Started run for {agentKey}")

- [ ] **Step 3: Mode `<select>` in composer**

Adjacent to textarea:

```tsx
<select value={mentionMode} onChange={...} aria-label="Mention mode">
  <option value="agent">Agent</option>
  <option value="chat">Chat</option>
</select>
```

Tooltips/titles per spec.

- [ ] **Step 4: @ autocomplete**

On `@` in textarea:
- Show dropdown of project agent keys (from `useAgents`, filter by project if available)
- Insert `@tech_lead` at cursor on select
- Keys: preset key, slugified name

Minimal v1: simple filtered list below textarea; no full combobox library required.

- [ ] **Step 5: Pass `mentionMode` on submit**

Only send when body contains `@` (optional optimization) or always send default `agent`.

- [ ] **Step 6: Run frontend tests**

Run: `make web-test`

- [ ] **Step 7: Commit**

```bash
git add web/src/lib/schemas/ticket.ts web/src/features/tickets/
git commit -m "$(cat <<'EOF'
Add Agent/Chat mode and @mention picker to ticket comment composer.
EOF
)"
```

---

### Task 8: End-to-end verification

**Files:**
- Modify (optional): `e2e/smoke/` or manual test checklist

- [ ] **Step 1: Manual smoke**

1. Open ticket with repo attached
2. Post `@tech_lead fix the README` with mode **Agent**
3. Verify: run appears in Runs tab, Live console streams, ticket status unchanged after completion
4. Inspect worktree `.agent/context.md` — human request first, no full description
5. Verify `.agent/ticket.json` exists during run
6. Post `@pm what is the status?` with mode **Chat** — `respond_to_mention` run, status unchanged

- [ ] **Step 2: Regression**

- Run Agent button still uses profile `full` and normal workflow
- Comment without `@mention` does not start a run
- `auto_start_runs = false` in config — human mention still starts run

- [ ] **Step 3: Full test suite**

Run: `make test-unit`

Expected: pass

- [ ] **Step 4: Final commit (if any doc touch-ups)**

Update spec status from Draft → Implemented if desired.

---

## Dependency graph

```text
Task 1 (migration + domain)
  → Task 2 (run_service)
    → Task 4 (job_worker)
    → Task 5 (orchestrator)
    → Task 6 (comments API)
  → Task 3 (context_builder) — can parallel with Task 2 after Task 1
Task 7 (web) — after Task 6 API shape is stable
Task 8 — last
```

---

## Out of scope (v1)

- Multiple `@mentions` per comment
- LLM routing between Agent/Chat
- Live mid-run context refresh
- Workflow toggle for human Agent runs
- Assignee change on mention-run

---

## Spec cross-check

- [x] Same worktree pipeline as Run Agent for Agent mode
- [x] Profiles on existing job types (no new job type)
- [x] Human mention always starts run (explicit intent)
- [x] No status transition on human Agent done
- [x] One mention per comment in v1
