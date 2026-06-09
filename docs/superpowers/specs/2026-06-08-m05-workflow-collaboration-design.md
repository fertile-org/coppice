# M05 — Workflow & Collaboration Design Spec

**Date:** 2026-06-08  
**Status:** Draft — pending user review  
**Product:** Coppice — grow an agent team from shared roots.

**Depends on:** M02 (board, tickets, comments, agents), M03 (job queue, runs, result contract), M04 (WebSocket events, live session)  
**Milestone doc:** `docs/milestones/M05-workflow-and-collaboration.md`

## Purpose

M05 delivers inter-agent coordination: strict status gates, comment-based `@mentions`, clarification/resume, per-status assignment control, and the human final review gate. This is the milestone where Coppice stops being “one agent, one manual run” and becomes a coordinated agent team on a ticket.

CI proves the full collaboration slice via **MockProvider** on the same orchestration path as real OpenCode agents — not a shortcut test harness.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Primary success story | Full mock pipeline in CI; MockProvider behaves like a real agent |
| M05 scope | **B** — PM → Engineer → `@mention` clarification → resume → Wait for Final Review → human Final Approve (no Review/QA roles in CI smoke) |
| Run trigger | `auto_start_runs` config: `false` locally, `true` in CI compose |
| Assignee gate | **No assignee → no run** (ever) |
| First assignment | Human assigns the first agent only; no `on_ticket_created` auto-assign |
| Status authority | **Workflow authoritative** — server applies column/assignee; agent `nextStatus` is **ignored** |
| Workflow config | **No YAML rule file** for M05 — transitions live in `WorkflowService` (Rust) |
| Status gates | `Backlog → Ready → In Progress → In Review → In QA → Wait for Final Review → Done` |
| Next assignee | Agent contract field `assignTo` (agent key); PM recommends engineer/researcher/FE/etc. |
| Human review at PM | `auto_assign` **per status** with `default = true`, `backlog = false` |
| Not supported | Hybrid “auto-assign then human override before run” — when `auto_assign` is true, assignment applies immediately and may auto-start |

---

## Core product model

### Status gates (strict)

```text
Backlog → Ready → In Progress → In Review → In QA → Wait for Final Review → Done
                                                              ↑
                                                    human Final Approve only

Blocked — side branch from any status; human or agent resolution returns to prior gate
```

Rules:

1. Every status change goes through `WorkflowService::resolve_transition`.
2. Illegal jumps are rejected (e.g. `Backlog → Done`, `In Progress → Done`).
3. `Wait for Final Review → Done` only via `POST /api/tickets/:id/final-approve`.
4. Agent `nextStatus` in the result contract is **not applied** (may remain in templates for agent reasoning; server logs at debug if present).

### Two control flags

| Flag | Scope | Purpose |
|------|-------|---------|
| `auto_start_runs` | Global | When assignee is set and a job is enqueued, start the run without human clicking **Run Agent** |
| `auto_assign` | Per status (default + overrides) | When agent returns `assignTo`, apply immediately vs store as pending recommendation |

These are independent: a human can assign manually at any time; `auto_start_runs` only affects whether a queued job executes without a button click.

### Configuration (M05)

```toml
[workflow]
auto_start_runs = false   # true in CI compose override

[workflow.auto_assign]
default = true
backlog = false           # review PM output + who to assign next
# Optional future overrides, e.g.:
# wait_for_final_review = false
# blocked = false
```

Resolution: `auto_assign.effective(status)` returns `backlog` override if set, else `default`.

**Default behavior:**

- **Backlog (`auto_assign = false`):** PM enriches ticket; gate may move to `Ready`; `assignTo` stored as **pending recommendation**; human reviews PM comment and assigns next agent manually.
- **All other statuses (`auto_assign = true`):** gate moves + `assignTo` applied immediately if agent exists; optional `auto_start_runs` queues next job.

---

## Representative flows (cases 1–4)

### Case 1 — PM enriches, human picks implementer

```text
Backlog
  human assigns PM
  PM runs → succeeded
  gate: Backlog → Ready
  assignTo: "engineer" (or "researcher") → pending recommendation (backlog auto_assign=false)
  human reviews PM comment + recommendation badge
  human assigns Engineer (or Researcher — may differ from PM pick)
  Ready → In Progress (on run start)
  Engineer runs → succeeded → In Review
  … pipeline continues with auto_assign=true …
  → Wait for Final Review
  human Final Approve → Done
```

### Case 2 — Direct engineer path

```text
Backlog
  human assigns Engineer (skips PM)
  on run start: Backlog → In Progress
  Engineer runs → succeeded → In Review
  … continues through gates with auto_assign=true …
```

### Case 3 — Mention without status move

```text
Backlog (or any status)
  human comments: "@engineer how do you think about the 2 options?"
  → ticket_mentions record + respond_to_mention job for Engineer
  → NO status change
  Engineer runs (respond_to_mention) → comment reply
  human manually assigns PM or others when ready
```

### Case 4 — PM assigns missing agent

```text
Backlog
  human assigns PM
  PM runs → assignTo: "frontend-engineer"
  agent lookup fails (no FE agent in project)
  → status Blocked, substatus + PM comment explaining missing role
  human creates FE agent or reassigns manually
```

---

## M05 CI smoke path (scope B)

Shorter than full gate chain; proves collaboration mechanics:

```text
Backlog → human assigns PM (CI may pre-assign + auto_start_runs=true)
PM → Ready + pending recommendation (or CI pre-assigns Engineer after PM)
Engineer → blocked + mentionAgents: [pm]
PM respond_to_mention → clarification answer
Engineer resume (work_on_ticket) → Wait for Final Review
human Final Approve → Done
```

MockProvider fixtures drive each step; same `job_worker` → `provider.run()` → `finish_with_apply` → `WorkflowService` path as OpenCode.

---

## Architecture

### Post-run orchestration pipeline

After a run completes, a single orchestrator runs in order:

```text
1. Parse result contract → comment body, intent, substatus (blocked/clarification)
2. WorkflowService.resolve_transition(context) → gate move, assignee, pending recommendation
3. MentionService.process_mentions → ticket_mentions + jobs (if mentionAgents or @ in comment)
4. JobService.enqueue_if_needed → respect assignee gate + auto_start_runs
5. EventBus → ticket.updated, comment.created, agent.mentioned
```

`finish_with_apply` is refactored: it no longer applies `nextStatus` from the contract.

### New / extended server modules

```text
server/src/
  services/
    workflow_service.rs      # gate validator + transition resolver
    mention_service.rs       # parse, persist, enqueue respond_to_mention
  domain/
    mention.rs
    workflow.rs              # TransitionContext, TransitionAction, PendingRecommendation
  workers/
    job_worker.rs            # extended job types + resume context
```

No `workflow.yaml`. Communication limits (clarification rounds, mention caps) are **hardcoded constants** in M05; extract to config later if needed.

### Transition context

```rust
TransitionContext {
  ticket_id,
  current_status,
  assignee_agent_id,
  agent_role,           // pm | engineer | researcher | frontend_engineer | ...
  agent_key,
  job_type,             // work_on_ticket | respond_to_mention
  run_outcome,          // succeeded | blocked
  contract,             // AgentRunResult (summary, assignTo, mentionAgents, …)
  project_agent_keys,   // for assignTo validation
  auto_assign_enabled,  // from config for current_status
}
```

```rust
TransitionAction {
  new_status: Option<TicketStatus>,
  new_assignee_id: Option<Uuid>,       // Some, None (unchanged), or explicit clear
  pending_recommendation: Option<PendingRecommendation>,  // assignTo when auto_assign=false
  substatus: Option<Substatus>,
  substatus_metadata: Option<Value>,
  enqueue_jobs: Vec<JobRequest>,
}
```

### Role-aware transitions (code, not config)

`WorkflowService` encodes gate logic by `(current_status, agent_role, job_type, run_outcome)`:

| Current | Role | Job | Outcome | Gate move | Notes |
|---------|------|-----|---------|-----------|-------|
| Backlog | PM | work_on_ticket | succeeded | → Ready | assignTo per auto_assign |
| Backlog | Engineer | work_on_ticket | start | → In Progress | on run **start**, not finish |
| Backlog | Engineer | work_on_ticket | succeeded | → In Review | direct path (case 2) |
| Ready | *implementer* | work_on_ticket | start | → In Progress | researcher, engineer, FE, etc. |
| Ready | *implementer* | work_on_ticket | succeeded | → In Review | |
| In Progress | *implementer* | work_on_ticket | succeeded | → In Review | |
| In Review | reviewer | work_on_ticket | succeeded | → In QA | M05 stub OK in tests |
| In QA | qc | work_on_ticket | succeeded | → Wait for Final Review | unassign agent |
| * | * | respond_to_mention | succeeded | none | comment only (case 3) |
| * | * | work_on_ticket | blocked + mentions | substatus waiting_for_agent | clarification |
| * | * | * | assignTo missing agent | → Blocked | case 4 |

Exact match table is unit-tested per row; CI smoke uses a subset.

### Pending recommendation

When `auto_assign` is false and contract includes `assignTo`:

```json
{
  "recommendedAgentKey": "engineer",
  "recommendedByAgentId": "<pm-uuid>",
  "recommendedAt": "<rfc3339>",
  "summary": "<optional one-liner from PM>"
}
```

Stored on ticket (`pending_assign_recommendation` JSONB column or `ticket_workflow_state` table).

Cleared when:

- Human assigns any agent (manual assign API)
- Human dismisses recommendation (optional API or implicit on assign)

UI shows badge: **“PM recommends: Engineer”** on ticket drawer.

---

## Result contract changes

### Server ignores

- `nextStatus` — not applied to board

### Server uses

| Field | Use |
|-------|-----|
| `status` (`done` / `blocked`) | Run outcome |
| `summary` | Agent comment body |
| `assignTo` | **New** — agent key for next assignee (validated against project agents) |
| `mentionAgents` | Create mentions + `respond_to_mention` jobs |
| `blockerType` + metadata | Substatus when blocked |
| `changedFiles`, `testsRun`, `blockers` | Comment enrichment (unchanged) |

### Agent templates / context

Update `.agent/context.md` contract section:

- Document `assignTo` instead of relying on `nextStatus` for board moves
- Keep `nextStatus` in examples as deprecated / ignored by server (or remove in M05 template pass)

---

## Job types (M05)

| Job type | When enqueued | Run behavior |
|----------|---------------|--------------|
| `work_on_ticket` | Human Run Agent, auto_start after handoff, resume after clarification | Full ticket work |
| `respond_to_mention` | `@agent` in human comment or `mentionAgents` in blocked contract | Answer question; no gate move |

Deferred: `review_ticket`, `qa_ticket` (gate transitions may stub in unit tests; not required in CI smoke).

### Clarification / resume

```text
Engineer work_on_ticket ends blocked, mentionAgents: ["pm"]
  → comment (intent: blocked)
  → ticket_mentions (pending, resume_agent = engineer)
  → substatus: waiting_for_agent, metadata { agentKey: "pm" }
  → job: respond_to_mention for PM
  → if auto_start_runs: PM runs

PM respond_to_mention ends succeeded
  → mention marked handled
  → clear waiting substatus
  → job: work_on_ticket (resume) for Engineer
  → clarification_round += 1; if > MAX_ROUNDS → waiting_for_human, no auto-resume
```

Resume context adds `## Resume` section to `.agent/context.md`: prior blocker + PM answer comment.

### Communication limits (hardcoded M05)

```rust
const MAX_CLARIFICATION_ROUNDS: u32 = 3;
const MAX_MENTIONS_PER_RUN: u32 = 2;
const MAX_AUTO_RESUME_COUNT: u32 = 3;
```

Exceeded → `substatus: waiting_for_human`, system comment, no further auto jobs.

---

## Mention system

### Triggers

1. **Human comment** with `@agent-key` → parse mentions, create `ticket_mentions`, enqueue `respond_to_mention`
2. **Agent contract** `mentionAgents` on blocked/done → same path

### `ticket_mentions` table

```text
id, ticket_id, comment_id, mentioned_agent_id, status (pending|handled|ignored),
resume_agent_id (nullable — for clarification), created_at, handled_at
```

### APIs

```text
POST /api/tickets/:id/final-approve     # human only; gate Wait for Final Review → Done
POST /api/tickets/:id/resolve-blocker     # non-capability blockers
POST /api/mentions/:id/ignore             # mark ignored → waiting_for_human
```

Existing: assign agent, run agent, comments CRUD.

---

## MockProvider (CI parity)

### Fixture resolution

Replace global `MOCK_AGENT_RESPONSE` as primary path:

```text
fixtures/agent-responses/{agent-key}/{job-type}.json
fixtures/agent-responses/{agent-key}/default.json
```

`MOCK_AGENT_RESPONSE` env remains as **test override only**.

### `AgentRunInput` extension

```rust
pub agent_key: String,
pub job_type: String,
```

Worker passes both; MockProvider selects fixture; OpenCode ignores for selection but same worker path.

### Example fixtures (scope B)

```text
pm/work_on_ticket.json           → done, assignTo: "engineer", enriched summary
engineer/work_on_ticket.json     → blocked, mentionAgents: ["pm"]
pm/respond_to_mention.json       → done, clarification answer
engineer/resume.json             → done (no assignTo; workflow moves to Wait for Final Review)
```

### CI compose

```yaml
environment:
  WORKFLOW_AUTO_START_RUNS: "true"
```

`auto_assign` uses defaults (`backlog=false`, `default=true`). Smoke script assigns PM, waits for Ready + recommendation, assigns Engineer, then pipeline runs unattended to Wait for Final Review.

---

## Frontend (minimal M05)

| Surface | Change |
|---------|--------|
| Board card | Substatus badge (`Waiting for PM`, `Blocked — missing agent`, etc.) |
| Ticket drawer | **Final Approve** when `wait_for_final_review` |
| Ticket drawer | **Pending recommendation** badge when `pending_assign_recommendation` set |
| Comments | `@agent` rendered as chips |
| Mentions | Ignore action on pending mentions (admin/owner) |

No workflow rule editor. No client-side gate logic — server is authoritative.

---

## Database delta

```text
ticket_mentions
tickets.pending_assign_recommendation JSONB NULL   # or ticket_workflow_state table
tickets.clarification_round INT DEFAULT 0
agent_runs.job_type TEXT NOT NULL                  # if not already on runs
```

Migration: `00N_workflow_collaboration.sql`

---

## Testing strategy

### Unit tests (`workflow_service.rs`)

- Gate validator rejects illegal jumps
- Case 1: Backlog + PM succeeded → Ready + pending recommendation when auto_assign false
- Case 2: Backlog + Engineer start → In Progress; succeeded → In Review
- Case 3: respond_to_mention → no status change
- Case 4: assignTo missing agent → Blocked
- auto_assign true at Ready → assignTo applied immediately
- Final Approve gate only from Wait for Final Review

### Integration tests

- Full scope B mock pipeline with `auto_start_runs=true`
- Mention → respond → resume → round limit → waiting_for_human
- Human comment `@engineer` creates job without status change

### E2E smoke (`e2e/smoke/m05-workflow.spec`)

1. Login → create ticket → assign repo → assign PM
2. With auto_start: PM completes → Ready + recommendation visible
3. Assign Engineer → blocked → PM answers → resume → Wait for Final Review
4. Final Approve → Done

### E2E full (local)

- `auto_start_runs=false`: manual Run Agent at each step
- Override PM recommendation with Researcher assign
- Ignore mention → escalates to human

---

## Out of scope (M05)

- YAML / UI workflow rule editor
- `on_ticket_created` auto-assign
- Full PM → TL → FE → BE → Review → QA chain in CI smoke
- M07 capability blocker guided unblock UI
- M06 knowledge injection into context
- Configurable communication limits (hardcoded only)
- `review_ticket` / `qa_ticket` job types in production worker (stubs in gate tests OK)

---

## Acceptance criteria (maps to milestone)

- [ ] Status gates enforced in code; agent `nextStatus` not applied
- [ ] `assignTo` with per-status `auto_assign` (default true, backlog false)
- [ ] Pending recommendation shown when backlog auto_assign false
- [ ] Mentions create jobs; clarification/resume works with round limits
- [ ] `respond_to_mention` does not move status (case 3)
- [ ] Missing `assignTo` agent → Blocked (case 4)
- [ ] Human Final Approve required before Done
- [ ] MockProvider role/job fixtures; CI smoke passes scope B pipeline
- [ ] `auto_start_runs` configurable; no assignee → no run

---

## References

- `docs/milestones/M05-workflow-and-collaboration.md`
- `docs/philosophy/final_agent_workspace_product_design.md` §8 (workflow), §9 (comments, mentions)
- `docs/superpowers/specs/2026-06-08-m03-agent-execution-design.md` (run pipeline baseline)

---

## Implementation plan

After user approves this spec, invoke **writing-plans** to produce `docs/superpowers/plans/2026-06-08-m05-workflow-collaboration.md`.
