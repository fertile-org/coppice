# M03 — Agent Execution Design Spec

**Date:** 2026-06-08  
**Status:** Superseded (repo model) — see [2026-06-08-m03-registered-repositories-design.md](./2026-06-08-m03-registered-repositories-design.md)  
**Implementation plan:** [docs/superpowers/plans/2026-06-08-m03-agent-execution.md](../plans/2026-06-08-m03-agent-execution.md) (obsolete for repo/clone sections; agent execution portions remain valid)
**Product:** Coppice — grow an agent team from shared roots.

**Depends on:** M01 Foundation (MockProvider, auth, Postgres), M02 Workspace & Board (tickets, agents, comments, repos)  
**Milestone doc:** `docs/milestones/M03-agent-execution.md`

## Purpose

M03 delivers the first end-to-end agent execution loop: Postgres-backed job queue, background worker, git worktrees, context packages, MockProvider runs, result-contract-driven ticket updates, and agent-authored comments. The ticket drawer gains a **Runs** tab and header actions **Run Agent** / **Stop**.

This spec captures design decisions from brainstorming (2026-06-08) and extends the milestone doc with concrete data models, pipeline stages, concurrency rules, and testing requirements.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Repo bootstrap | Lazy `git clone` from `repos.remote_url` when `/data/repos/{repo-id}/` is missing; fail clearly if URL empty |
| Result side effects | Apply `nextStatus` + agent comment; store `mentionAgents` on comment `mentions` only — no mention records or jobs (M05) |
| Run preconditions | Strict: ticket must have `assignee_agent_id` and linked repo with non-empty `remote_url` |
| Parallelism | Different agents may run in parallel on the same ticket |
| Worktree isolation | One worktree per `(ticket, agent)` — path `TICKET-{id}-{agent-slug}-{repo-slug}`, branch `agent/TICKET-{id}-{agent-slug}` |
| Same-agent overlap | Reject new run if `(ticket, agent)` already has a run in `queued` or `running` |
| Blocked results | Hybrid: `nextStatus` moves board column; when landed on `Blocked`, map `blockerType` → substatus + metadata |
| CI git repos | Tests `git init` a temp bare repo at runtime; set Coppice repo `remote_url` to `file://…` |
| UI layout | **A — Header actions:** Run Agent / Stop in drawer header; Runs tab is read-only history |
| Worker architecture | In-process Tokio worker(s) inside server binary; Postgres queue with `FOR UPDATE SKIP LOCKED` |

## Out of scope (unchanged from milestone)

- Live terminal streaming (M04)
- Workflow rule engine and mention jobs (M05)
- Knowledge injection into context (M06)
- Strict capability/sandbox enforcement (M07)
- Real CLI adapters (Claude Code, Codex, etc.)
- `signal_created` result handling (parser may accept; no-op in orchestration)

---

## Architecture overview

### Worker approach (selected)

**In-process Tokio worker (recommended and selected)**

- One or more background tasks spawned at server startup (`AGENT_WORKER_COUNT`, default `2`).
- Poll `agent_jobs` with `FOR UPDATE SKIP LOCKED`.
- Execute full pipeline inline: clone → worktree → context → provider → parse → apply.
- Single compose service; matches M01/M02 deploy model.

Rejected alternatives:

- **Separate worker binary** — extra deploy surface with no M03 benefit.
- **Synchronous run (no queue)** — blocks HTTP, no stop/retry, poor fit for M04+ CLI adapters.

### Monorepo delta

```text
server/migrations/003_agent_execution.sql
server/src/
  workers/job_worker.rs
  services/
    job_service.rs
    run_service.rs
    worktree_service.rs
    context_builder.rs
    result_contract.rs
  providers/mock.rs              # extended: optional stdout emission
  sandbox/permissive.rs          # placeholder until M07
  api/agent_runs.rs, jobs.rs
  domain/run.rs, job.rs          # optional thin domain types
deploy/Dockerfile.server         # + git CLI
deploy/docker-compose.yml        # + worktree_data, repo_data volumes
deploy/docker-compose.local.yml  # same volume/env delta
fixtures/agent-responses/        # existing; may add variants for tests
web/src/features/tickets/
  TicketRunsTab.tsx
  useAgentRuns.ts
e2e/smoke/m03-agent-run.mjs
```

### Run pipeline

```text
POST /api/tickets/:id/run-agent
  → validate assignee + repo + remote_url
  → reject if active run for (ticket_id, agent_id)
  → INSERT agent_run (queued) + agent_job (pending)

Worker claims job:
  → agent_run.status = running
  → ensure repo clone at GIT_REPOS_PATH/{repo-id}/
  → ensure worktree + branch for (ticket, agent)
  → write .agent/context.md in worktree
  → MockProvider.run({ context_path, agent_id, ticket_id })
  → parse AgentRunResult via result_contract.rs
  → apply ticket status/substatus + create agent comment
  → agent_run.status = succeeded | blocked | failed
  → agent_job.status = done | failed | cancelled
```

### Stop and retry

**Stop** (`POST /api/agent-runs/:id/stop`):

- Allowed when run status is `queued` or `running`.
- Sets job to `cancelled`, run to `cancelled`.
- M03 MockProvider is instant; no process kill yet. Hook reserved for M04.

**Retry** (`POST /api/agent-runs/:id/retry`):

- Creates a **new** `agent_run` + `agent_job` for the same ticket and agent.
- Previous run remains in history unchanged.
- Same preconditions and duplicate-run guard apply.

---

## Data model

Migration `003_agent_execution.sql`.

### `agent_runs`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| ticket_id | UUID FK → tickets NOT NULL | |
| agent_id | UUID FK → agents NOT NULL | |
| job_type | TEXT NOT NULL | `work_on_ticket` only in M03 |
| status | TEXT NOT NULL | `queued`, `running`, `succeeded`, `failed`, `blocked`, `cancelled` |
| sandbox_profile_id | TEXT NOT NULL | `permissive-default` |
| worktree_path | TEXT NULL | Set when worktree created |
| branch_name | TEXT NULL | e.g. `agent/TICKET-{id}-{agent-slug}` |
| error_message | TEXT NULL | On failure |
| started_at | TIMESTAMPTZ NULL | |
| ended_at | TIMESTAMPTZ NULL | |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | |

**Partial unique index** — enforce one active run per `(ticket_id, agent_id)`:

```sql
CREATE UNIQUE INDEX agent_runs_active_ticket_agent_idx
  ON agent_runs (ticket_id, agent_id)
  WHERE status IN ('queued', 'running');
```

Indexes: `(ticket_id, created_at DESC)`, `(agent_id)`.

### `agent_jobs`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| run_id | UUID FK → agent_runs NOT NULL | |
| job_type | TEXT NOT NULL | `work_on_ticket` |
| status | TEXT NOT NULL | `pending`, `processing`, `done`, `failed`, `cancelled` |
| attempts | INT NOT NULL DEFAULT 0 | |
| max_attempts | INT NOT NULL DEFAULT 3 | |
| available_at | TIMESTAMPTZ NOT NULL DEFAULT now() | |
| locked_at | TIMESTAMPTZ NULL | |
| locked_by | TEXT NULL | Worker id |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | |

Index for worker poll: `(status, available_at)` where `status = 'pending'`.

---

## Git and worktree lifecycle

### Paths

```text
/data/repos/{repo-id}/                                    ← lazy clone target
/data/worktrees/TICKET-{ticket-id}-{agent-slug}-{repo-slug}/
```

Environment variables:

- `GIT_REPOS_PATH` (default `/data/repos`)
- `WORKTREES_PATH` (default `/data/worktrees`)

Slug sanitization: lowercase, non-alphanumeric → `-`, collapse repeats, trim edges.

### Clone

On first run needing a repo:

1. If `{GIT_REPOS_PATH}/{repo-id}/` exists → use it.
2. Else if `repos.remote_url` is set → `git clone {remote_url} {path}`.
3. Else → fail run with clear error (no repo creation in Coppice).

### Worktree per (ticket, agent)

1. Compute path and branch from ticket id, agent name slug, repo name slug.
2. If worktree path exists → reuse (same agent re-run).
3. Else → `git worktree add -b {branch} {path}` from repo clone.
4. Update `tickets.branch_name` when worktree is first created for the assignee's run (or per-agent branch on run record).

Different agents on the same ticket get separate worktrees and branches — enables parallel FE + BE runs.

### Ticket fields

- `tickets.repo_id` — required for run (validated at API).
- `tickets.branch_name` — updated to reflect primary/current agent branch as documented in Metadata tab.
- Run record stores authoritative `worktree_path` and `branch_name` per execution.

---

## Context package (M03 scope)

Written to `{worktree}/.agent/context.md` before provider invocation.

Sections (M03 minimal):

1. **Current task** — ticket title, description, status, substatus
2. **Agent role** — name, role, skills, responsibilities, system prompt
3. **Sandbox note** — permissive profile placeholder; “return blocked if capability missing” instruction
4. **Expected output contract** — JSON shape summary (`done` / `blocked` variants from product design §17)

Excluded until later milestones: comments history, artifacts, knowledge, secrets, capabilities list.

---

## Result contract

Parser in `result_contract.rs` extends existing `AgentRunResult` enum in `providers/mod.rs`.

### `done`

| Field | Action |
|-------|--------|
| `nextStatus` | Map to board column; PATCH ticket status |
| `summary` | Agent comment body (markdown) |
| `changedFiles`, `testsRun` | Append as markdown lists in comment |
| `mentionAgents` | Comment `mentions` JSON array (agent keys/names) |
| `blockers` | Include in comment if non-empty |
| Comment intent | `implementation_done` |

### `blocked`

| Field | Action |
|-------|--------|
| `nextStatus` | Move ticket to column (e.g. `Blocked`, `Waiting for PM`) |
| `blockerType` | When status is `Blocked`, map to substatus (see table below) |
| `requiredCapabilities`, `requiredSecrets`, etc. | Substatus metadata when applicable |
| `summary` | Agent comment body |
| `mentionAgents` | Comment `mentions` only |
| Comment intent | `blocked` |
| Run status | `blocked` |

### `blockerType` → substatus mapping

| blockerType | Substatus | Metadata |
|-------------|-----------|----------|
| `missing_capability` | `blocked_by_missing_capability` | `{ capability: first requiredCapabilities[] }` |
| `missing_secret` | `blocked_by_missing_secret` | `{ secretKey: first requiredSecrets[] }` |
| `permission` | `blocked_by_permission` | optional `reason` |
| `needs_human` | `waiting_for_human` | optional `reason` from summary |
| other | `blocked_by_error` | `reason` from summary |

Run status `blocked` vs `failed`: contract parse success with `status: blocked` → run `blocked`; provider/IO/git errors → run `failed`.

### Mentions (M03)

Store on comment only. Do **not** create `mentions` table rows or enqueue follow-up jobs (M05).

---

## API

All routes session-authenticated; mutations require CSRF.

```text
POST  /api/tickets/:id/run-agent
      → 201 { run: AgentRun }
      → 400 if missing assignee/repo/remote_url
      → 409 if active run for (ticket, assignee agent)

GET   /api/tickets/:id/runs
      → 200 { runs: AgentRun[] }  ordered by created_at desc

GET   /api/agent-runs/:id
      → 200 { run: AgentRun }

POST  /api/agent-runs/:id/stop
      → 200 { run: AgentRun }
      → 409 if not queued/running

POST  /api/agent-runs/:id/retry
      → 201 { run: AgentRun }  new run

GET   /api/agent-jobs
      → 200 { jobs: AgentJob[] }  admin-only debug
```

### AgentRun JSON (API)

```json
{
  "id": "uuid",
  "ticketId": "uuid",
  "agentId": "uuid",
  "jobType": "work_on_ticket",
  "status": "running",
  "sandboxProfileId": "permissive-default",
  "worktreePath": "/data/worktrees/TICKET-…",
  "branchName": "agent/TICKET-…-fe",
  "startedAt": "…",
  "endedAt": null,
  "createdAt": "…",
  "errorMessage": null
}
```

CamelCase in JSON; Rust snake_case internally.

---

## UI

Layout **A — Header actions** (brainstorming 2026-06-08).

### Ticket drawer header

- **Run Agent** (primary) — visible when ticket has assignee + repo; disabled with tooltip if preconditions missing.
- **Stop** — visible when assignee has active run (`queued`/`running`) on this ticket.
- **Close** — unchanged.

### Runs tab (new)

Fourth tab: Description | Comments | **Runs** | Metadata.

Read-only list:

- Status pill (queued, running, succeeded, failed, blocked, cancelled)
- Agent name
- Relative timestamps (`startedAt`, `endedAt`)
- Branch + worktree path (mono, truncated with title tooltip)
- Optional: expand row for `errorMessage`

Poll/refetch every few seconds while any run on ticket is active.

### Metadata tab delta

Show `branch_name`, worktree path from latest run or ticket when present.

### Web hooks

- `useAgentRuns(ticketId)` — list query + invalidation on run-agent/stop/retry
- `useRunAgent`, `useStopRun`, `useRetryRun` — mutations

---

## Docker Compose delta

Both `deploy/docker-compose.yml` and `deploy/docker-compose.local.yml`:

```yaml
server:
  volumes:
    - artifact_data:/data/artifacts
    - worktree_data:/data/worktrees
    - repo_data:/data/repos
  environment:
    AGENT_DEFAULT_PROVIDER: mock
    GIT_REPOS_PATH: /data/repos
    WORKTREES_PATH: /data/worktrees
    AGENT_WORKER_COUNT: "2"

volumes:
  worktree_data:
  repo_data:
```

`Dockerfile.server`: install `git` package.

Local stack (`docker-compose.local.yml`): same volumes; host-run Vite unchanged.

---

## MockProvider extension

- Continue reading JSON from `fixtures/agent-responses/{MOCK_AGENT_RESPONSE}.json`.
- Optionally emit stdout lines to a file under artifacts dir (`agent_result` artifact type metadata) to prepare M04 log capture — no WebSocket streaming in M03.
- Respect cancellation: if run cancelled before provider returns, skip apply step.

---

## Sandbox

`sandbox/permissive.rs` — placeholder profile `permissive-default`:

- Wide command/path allowlist (documented, not enforced strictly until M07).
- Referenced in run record and context package.
- No capability or secret injection.

---

## Error handling

| Failure | Run status | Ticket | User-visible |
|---------|------------|--------|--------------|
| Clone failed | `failed` | unchanged | System comment optional; error in run detail |
| Worktree git error | `failed` | unchanged | `errorMessage` on run |
| Provider/fixture error | `failed` | unchanged | `errorMessage` |
| Invalid result JSON | `failed` | unchanged | `errorMessage` |
| Cancelled | `cancelled` | unchanged | — |
| Blocked contract | `blocked` | status/substatus updated | agent comment |

Integration tests use `MockProvider` only; no real CLI in CI.

---

## Testing strategy

### Unit tests

- `context_builder` — expected markdown sections present
- `result_contract` — all JSON variants from product design §17 (done, blocked types; signal_created parse-only)
- Job state machine transitions
- Worktree path + branch slug sanitization
- `blockerType` → substatus mapping

### Integration tests

Setup: temp bare repo via `git init --bare` + `git push` or `file://` remote; create Coppice repo with that URL.

| Scenario | Assert |
|----------|--------|
| Happy path | run-agent → job processed → comment + status In Review |
| Blocked fixture | substatus + blocked comment |
| Lazy clone | repo path created on disk |
| Worktree | directory exists; branch in clone |
| Stop | cancels queued/running run |
| Retry | new run id; same ticket |
| Duplicate same agent | 409 on second run-agent |
| Parallel agents | FE + BE runs both succeed with distinct worktrees |

### E2E smoke (`e2e/smoke/m03-agent-run.mjs`)

1. Login → create/open ticket → assign mock-configured agent → link repo
2. Run Agent (API or UI) → wait for succeeded
3. Agent comment in Comments tab
4. Runs tab shows completed run

Full E2E (local): stop mid-run, retry after failure, metadata shows worktree/branch.

---

## Acceptance criteria

Aligned with `docs/milestones/M03-agent-execution.md`:

- [ ] Mock agent run completes end-to-end on a ticket
- [ ] Result contract drives comment + status/substatus update
- [ ] Worktree and branch created per (ticket, agent)
- [ ] Stop and retry work via API and UI
- [ ] All automated tests use MockProvider only
- [ ] CI smoke E2E passes

---

## Implementation order (recommended)

1. Migration + domain types
2. `worktree_service` + `context_builder` + unit tests
3. `result_contract` + unit tests
4. `run_service`, `job_service`, `job_worker` + integration tests
5. API routes + integration tests
6. MockProvider stdout extension (minimal)
7. Compose/Dockerfile delta
8. Web: Runs tab, header actions, hooks
9. E2E smoke script + CI wiring
10. Update `AGENTS.md` / `docs/architecture.md` pointers

---

## References

- `docs/milestones/M03-agent-execution.md`
- `docs/philosophy/final_agent_workspace_product_design.md` — §7, §10, §11, §16, §17
- `docs/philosophy/final_agent_workspace_framework_selection.md` — job queue, git CLI
- `docs/web/DESIGN.md` — UI tokens for Runs tab
- Brainstorming canvas: `m03-runs-tab-layout` (layout A selected)
