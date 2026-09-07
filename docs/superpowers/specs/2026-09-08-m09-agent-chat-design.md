# M09 Agent Chat Design

**Status:** Accepted design gate (docs-only; implement after M07 + M08)  
**Date:** 2026-09-08  
**Owner/reviewer:** Technical Lead  
**Milestone:** [M09 — Agent Chat](../../milestones/M09-agent-chat.md)

## Decision summary

Coppice adds a **first-class, human-owned chat surface** for exploratory human↔agent conversation outside ticket threads. Sessions and messages are durable, SPA-visible, and auditable. Each human turn enqueues one `agent_runs` row with `job_type = chat_turn` and `context_profile = conversation`, owned by a **chat session XOR a ticket** — never a synthetic ticket.

Chat cwd is either the bound registered repo’s `local_path` or `$WORKTREES_PATH/chat/{session_id}/`. Write tools are **enforced denied** (fail-closed for connectors that cannot enforce allowlists). Streaming reuses `GET /ws/agent-runs/:runId/live`. Compact server-validated actions route lasting work onto the board (`create_ticket`) or M06 Knowledge Inbox as **pending only** (`create_knowledge`), or mark the session `cutoff`.

Ticket `@mention` Chat mode (`human_chat` / `respond_to_mention`) is unchanged and must not create `chat_sessions` rows.

**Ordering:** do not implement server/web for this milestone until M07 and M08 are accepted. This design gate may land in parallel.

## Goals

- Human-inspectable chat sessions (list + transcript + composer) scoped to a project.
- Durable `chat_sessions` / `chat_messages` with soft archive; hard delete out of scope.
- First-class chat turns as `agent_runs` without faking tickets or breaking ticket-run uniqueness.
- Distinct `conversation` profile and `chat_turn` job — not overloaded `human_chat` / `respond_to_mention`.
- Read-oriented tooling under an explicit cwd policy; write denial enforced.
- Actions: `create_ticket`, `create_knowledge` (pending), `cutoff` — no workflow/board moves from chat.
- Selective SPA reuse of `web/src/opencode-session/` for streamed assistant parts, wired like `LiveSession.tsx`.

## Non-goals

- Implementing M09 code before M07/M08 acceptance.
- Agent↔agent chat rooms / group chat / proactive agent DMs.
- Replacing ticket `mentionMode=chat` (`human_chat`).
- Write-enabled coding from chat (use tickets + `human_agent` / Run Agent).
- Auto-approved knowledge from chat.
- A second full token-stream protocol beside run live WS.
- Stealing M07 sandbox/secrets/signals or M08 connector-CLI scope (chat write-denial sits **on top of** future sandbox).

## Philosophy amendment (human↔agent only)

Product design historically forbade hidden chats. M09 **amends** observability language for **human↔agent** exploration only. Exact wording lands in `docs/philosophy/final_agent_workspace_product_design.md` as part of this design-gate set:

| Channel | Role after M09 |
|---------|----------------|
| Ticket comments + `@mention` Agent/Chat | Official **ticket-bound** work and **inter-agent** protocol (unchanged) |
| Agent Chat sessions | Official **human↔agent** exploratory conversation — visible in SPA/API, auditable |

Hard rules retained:

- Sessions are never a hidden side channel between agents.
- Agents do **not** use chat sessions to talk to each other.
- Chat is not a second Kanban; durable execution stays on tickets; durable memory still requires M06 Inbox approval.

## Data model

### `chat_sessions`

| Column | Purpose |
|--------|---------|
| `id` | UUID PK |
| `project_id` | FK → projects |
| `owner_user_id` | Human who owns the session |
| `agent_id` | Bound agent for turns |
| `repo_id` | Optional FK → repos (bound cwd) |
| `status` | `active` \| `archived` \| `cutoff` |
| `created_at`, `updated_at` | Audit / list ordering |

Constraints: project member ownership enforced in service; one bound agent per session in v1.

### `chat_messages`

| Column | Purpose |
|--------|---------|
| `id` | UUID PK |
| `session_id` | FK → chat_sessions ON DELETE CASCADE |
| `seq` | Monotonic per-session sequence (unique `(session_id, seq)`) |
| `role` | `human` \| `agent` \| `system` |
| `body` | Message text |
| `agent_run_id` | Optional FK → agent_runs (turn that produced/consumed this message) |
| `action_metadata` | JSONB — validated action results (`create_ticket`, `create_knowledge`, `cutoff`) |
| `created_at` | Timestamp |

### `agent_runs` ownership (chat XOR ticket)

**Fact today:** `agent_runs.ticket_id` is `NOT NULL`; active uniqueness is `(ticket_id, agent_id)` where status ∈ (`queued`, `running`) (`003_agent_execution.sql`). Chat cannot fake a ticket.

**Locked migration shape:**

1. Create `chat_sessions` / `chat_messages` first.
2. Alter `agent_runs`:
   - `ticket_id` → **nullable**
   - add `chat_session_id UUID NULL REFERENCES chat_sessions(id) ON DELETE CASCADE`
   - add `chat_message_id UUID NULL REFERENCES chat_messages(id) ON DELETE SET NULL` (human message that triggered the turn)
   - CHECK XOR ownership:

```sql
CHECK (
  (ticket_id IS NOT NULL AND chat_session_id IS NULL)
  OR (ticket_id IS NULL AND chat_session_id IS NOT NULL)
)
```

3. Active uniqueness:
   - keep `agent_runs_active_ticket_agent_idx` on `(ticket_id, agent_id) WHERE status IN ('queued','running') AND ticket_id IS NOT NULL`
   - add `agent_runs_active_chat_session_agent_idx` on `(chat_session_id, agent_id) WHERE status IN ('queued','running') AND chat_session_id IS NOT NULL`

4. Extend `context_profile` CHECK to include `'conversation'`.

5. Optional: `tickets.source_chat_session_id UUID NULL REFERENCES chat_sessions(id) ON DELETE SET NULL` (+ index) for `create_ticket` provenance.

**Domain ripple (implement later):** `AgentRun.ticket_id: Uuid` → `Option<Uuid>`; chat enqueue must **not** reuse `RunService::start_run_for_agent` as-is (that path assumes ticket + repo readiness). Add `start_chat_turn` (or equivalent) that inserts the XOR-owned run + job.

### Soft archive

- `PATCH` session → `archived` / reopen to `active`.
- `cutoff` is a distinct status (see actions); reject further human `POST …/messages` until human reopens to `active`.
- Hard delete out of scope for v1.

## API

All routes require authenticated project membership. Mutations require CSRF (`X-CSRF-Token`). Knowledge/ticket side-effects are **server-applied from validated agent results**, not a privilege escalation for arbitrary clients.

```text
GET    /api/chat/sessions
POST   /api/chat/sessions              # { agentId, repoId? }
GET    /api/chat/sessions/:id
PATCH  /api/chat/sessions/:id          # archive / reopen (status)
POST   /api/chat/sessions/:id/messages # { body } → human message + enqueue chat_turn
GET    /api/chat/sessions/:id/messages
POST   /api/chat/sessions/:id/cutoff   # human-initiated cutoff (or agent action)
```

**Message POST semantics:**

1. Reject if session status ≠ `active`.
2. Insert human `chat_messages` row (next `seq`).
3. Insert `agent_runs` with `ticket_id = NULL`, `chat_session_id`, `chat_message_id`, `job_type = chat_turn`, `context_profile = conversation`, bound `agent_id`.
4. Enqueue `agent_jobs` for that run.
5. Return message + `runId` so the SPA can subscribe to live WS immediately.

**List/get:** keyset pagination consistent with other list APIs; include latest message preview and active-run id when present.

## Runs: `chat_turn` + coexistence with ticket runs

| Path | `job_type` | `context_profile` | Ownership |
|------|------------|-------------------|-----------|
| Run Agent / workflow | `work_on_ticket` | `full` (or `human_agent` for mention Agent) | `ticket_id` set |
| Ticket `@mention` Chat | `respond_to_mention` | `human_chat` | `ticket_id` set; **no** `chat_sessions` row |
| Agent consultation | `respond_to_mention` | `full` | `ticket_id` set |
| **Agent Chat turn** | **`chat_turn`** | **`conversation`** | **`chat_session_id` set** |

Regression lock: `mentionMode=chat` on ticket comments continues to create only `respond_to_mention` + `human_chat` with required `ticket_id`. Integration tests must assert **zero** `chat_sessions` rows for that path.

On success: persist agent `chat_messages` row linked to the run; attach `action_metadata` when actions fire. On failure/blocker: persist system or agent error body as specified by implementer (must be visible in transcript); never silent.

Git finalize / auto-commit paths that key on `work_on_ticket` must **skip** `chat_turn` (no auto-commit from chat scratch or repo cwd).

## Context profile `conversation`

New exhaustive match arm in `context_builder` and `job_worker` (today: `full` | `human_agent` | `human_chat` only).

**Include:**

- Session metadata (id, project, bound agent, optional repo hint/path).
- Bound agent soul / role.
- Recent chat transcript under a deterministic token budget (human message that triggered the turn first / most salient).
- **Chat action contract** (not ticket done/blocked/assignTo).
- Optional project/repo path hints for orientation.

**Exclude:**

- Knowledge retrieval (same as `human_chat` / non-Full — worker must not call retrieval).
- Ticket comment thread, workflow status, `.agent/ticket.json` ticket snapshot requirement.
- Full consultation branch and HumanAgent workflow branches.
- Ticket result-contract fields (`assignTo`, `splitTickets`, `updatedDescription`, board `nextStatus`).

Do **not** reuse Full consultation or HumanAgent builders by parameterizing them — add `build_conversation_context`.

## Cwd resolver (`chat_cwd`)

```text
if session.repo_id is Some:
  cwd = registered repos.local_path   # read-oriented
else:
  cwd = $WORKTREES_PATH/chat/{session_id}/
  create directory on first turn if missing
```

Rules:

- Never call `compute_paths(ticket_id)` / never attach to `TICKET-*` worktrees.
- Store resolved path on the run’s `worktree_path` for live/reattach (reuse existing column name; semantics = cwd for the turn).
- Bound-repo path is the registered checkout — **read-only policy** via write denial, not a separate clone.
- Scratch tree: create as empty directory (git init **not** required in v1); **no auto-commit**; **no push**.

### Scratch lifecycle

| Phase | Policy |
|-------|--------|
| Create | On first `chat_turn` for unbound session |
| During turn | Provider cwd = scratch; writes should still be denied by connector policy |
| After turn | Retain directory for resume/reattach within session lifetime |
| Cleanup | **Deferred** — v1 = operator/manual or best-effort janitor later; document disk-growth risk. No silent `rm -rf` of unrelated paths. |

## Write-tool denial (hard)

**Fact:** connectors currently bypass permissions (Claude `--allowedTools` includes Write/Edit; Cursor `--force`; Codex `--dangerously-bypass-approvals-and-sandbox`; Kilo `--auto`). Prompt-only denial (HumanChat) is insufficient.

**Decision:**

1. Orchestrator/job_worker sets a run capability flag for `conversation` / `chat_turn` (e.g. `read_only_tools = true` on `AgentRunInput`).
2. **Fail closed:** if the selected provider cannot enforce a read-only allowlist, refuse the turn with a clear error (`missing_capability` / chat-specific blocker) — do not start the vendor CLI with write-capable flags.
3. Per connector that **is** chat-capable: pass a read-only tool allowlist (read / grep / glob / list / equivalent). Deny write, edit, multi-edit, notebook edit, apply_patch, TodoWrite-as-mutation, Task-that-spawns-writers, Bash (mutating shell), git commit/push.
4. `MockProvider`: fixture + unit matrix assert denial / capability path (including a fixture that would have written, proving the orchestrator/provider gate).
5. Violations observed mid-run (tool event write) → failed turn / blocker, **never** silent no-op.

### Chat-capable connectors (v1)

| Connector | Enforceable read-only? | v1 chat support |
|-----------|------------------------|-----------------|
| `mock` | Yes (orchestrator/fixture) | **Supported** (CI) |
| `claude-code` | Yes — narrow `--allowedTools` (omit Write/Edit/MultiEdit/NotebookEdit/Bash/TodoWrite/Task; keep Read/Glob/Grep; decide WebFetch/WebSearch as read-only OK) | **Supported** when enabled |
| `cursor` | No clean allowlist today (`--force`) | **Unsupported** — fail closed |
| `codex` | No (`--dangerously-bypass-…`) | **Unsupported** — fail closed |
| `kilo-code` | No (`--auto`) | **Unsupported** — fail closed |
| `opencode` | No proven read-only allowlist in-tree today | **Unsupported** — fail closed until implementer proves flags and updates this matrix |

M07 sandbox is defense-in-depth, not a substitute for this matrix. Expanding chat-capable connectors is a follow-up after enforceable flags exist.

## Streaming

**Primary (locked):** each turn is an `agent_run`. Clients subscribe to existing `GET /ws/agent-runs/:runId/live` (`LiveMessage`: Frame / Snapshot / Event / Heartbeat / End). Reconnect via run registry buffer + artifact replay — same contract as M04 / `LiveSession.tsx`.

**Secondary (optional):** thin `/ws/events` types `chat.message.created`, `chat.turn.finished` for session-list badges / unread hints. Not a second token stream; SPA may also poll session list.

## Action contracts (server-validated)

Distinct from ticket result JSON. Chat turn result schema (names locked):

| Action | Server behavior |
|--------|-----------------|
| `create_ticket` | `TicketService::create` with title/description/optional repo; set `source_chat_session_id`; link ticket id in message `action_metadata` |
| `create_knowledge` | Service-level pending insert (same trust as extractor / `create_manual` → **pending**); provenance `source_type = chat_session` (+ source ids / `source_run_id`); **never** auto-approve; **must not** require the human to call admin-only `POST /api/knowledge` |
| `cutoff` | Session status → `cutoff`; reject further message POSTs until reopen |

No workflow status transitions from chat. No board moves. No `assignTo` / `splitTickets` application from chat results.

**Authz:** session create/message requires authenticated project member. Side-effects run inside the server from validated agent JSON — keep `POST /api/knowledge` admin-only; extend `KnowledgeSourceType` with `chat_session` for provenance.

**Apply path:** new `apply_chat_result` (name flexible) — do not route through `apply_agent_result` / `apply_consultation_result`.

### Result JSON sketch (illustrative)

```json
{
  "status": "done",
  "summary": "Short reply shown as the agent message body.",
  "actions": [
    {
      "type": "create_ticket",
      "title": "…",
      "description": "…",
      "repoId": null
    },
    {
      "type": "create_knowledge",
      "title": "…",
      "content": "…",
      "knowledgeType": "coding_convention",
      "scope": "project"
    },
    { "type": "cutoff" }
  ]
}
```

Unknown action types fail the turn. Empty `actions` is valid (reply-only).

## UI reuse

New Chat feature area (`web/src/features/chat/*`):

- Session list (project-scoped) + transcript + composer (pick agent + optional repo).
- Wire live turns like `web/src/features/runs/LiveSession.tsx` to `/ws/agent-runs/:runId/live`.
- Reuse from `web/src/opencode-session/`: `sync/reduce-event` + types, `SessionView` / text + reasoning parts, theme CSS where it fits streamed assistant turns; Read/Grep/Glob tool chrome is fine for read-only turns.

**Do not:**

- Adopt OpenCode TUI identity / footer as the product shell.
- Use Write/Edit tool chrome as primary UX for chat (deny path may still render a failed tool event).
- Use `AgentResultCard` ticket-contract UI as the chat outcome shell — prefer compact action result chips (ticket link, inbox pending link, cutoff banner).

## Suggested modules (implement later)

```text
server/src/domain/chat_session.rs
server/src/domain/chat_message.rs
server/src/services/chat_service.rs
server/src/services/chat_cwd.rs
server/src/api/chat.rs
# reuse job_worker + providers with conversation / chat_turn branches

web/src/features/chat/*
```

Migration: next unused number after current head (today `019_…`; at implement time use the next free id).

## Coexistence and regression locks

| Scenario | Expected |
|----------|----------|
| Ticket comment `mentionMode=chat` | `respond_to_mention` + `human_chat` only; **no** `chat_sessions` row |
| Ticket comment `mentionMode=agent` | `work_on_ticket` + `human_agent` unchanged |
| Full Run Agent | `full` + knowledge retrieval unchanged |
| Chat `create_knowledge` | Inbox **pending**; approve still required; retrieval still Full-only |
| Chat cwd | Never `TICKET-*` worktree paths |
| Unsupported connector for chat | Clear fail-closed error; no write-capable spawn |

## Testing strategy (design-gate acceptance for plan)

### Unit

- Cwd resolver: bound repo vs `$WORKTREES_PATH/chat/{session_id}/`
- Write-denial / chat-capable matrix (mock + claude allowlist; cursor/codex/kilo refuse)
- Action parsers: `create_ticket`, `create_knowledge` (pending only), `cutoff`
- Context builder: `conversation` excludes knowledge retrieval and ticket thread; includes transcript + chat contract
- XOR CHECK / active unique index behavior (SQL or service-level conflict)

### Integration

- Create session → post message → mock `chat_turn` → agent message persisted + `agent_run_id` linked
- `create_ticket` → ticket exists with `source_chat_session_id`
- `create_knowledge` → inbox pending; not approved; `POST /api/knowledge` remains admin-only
- Cutoff rejects further POSTs; reopen restores
- Ticket `mentionMode=chat` → `human_chat` / `respond_to_mention` only (no chat_sessions)

### Smoke (post-implement)

- `make e2e-smoke-m09` (or extend existing) — session create, one mock turn, cutoff on default Compose (`MockProvider`)

## Alternatives rejected

1. **Synthetic tickets for chat.** Would pollute the board and fight `ticket_id NOT NULL` uniqueness semantics.
2. **Reuse `human_chat` / `respond_to_mention` for SPA chat.** Couples ticket comments to session UX; breaks “no ticket thread” and action contract goals.
3. **Prompt-only write denial.** Connectors already bypass permissions; fails acceptance.
4. **Support all connectors in v1 chat.** Cursor/Codex/Kilo lack enforceable allowlists; fail-closed is safer than pretend denial.
5. **Auto-approve knowledge from chat.** Violates M06 inbox trust.
6. **Second token WebSocket for chat.** Duplicates M04 live path; reconnect/replay already exist on run WS.

## Risks

| Risk | Mitigation |
|------|------------|
| Schema blast radius around nullable `ticket_id` | Careful migration; update all SQL/`Option` assumptions; dual partial unique indexes |
| Connector heterogeneity | Document chat-capable allowlist; fail closed |
| Admin-only knowledge API vs agent-applied create | Service path + audit; do not weaken HTTP authz |
| Scratch cwd disk growth | Deferred cleanup documented; operator guidance |
| Scope creep into write-enabled coding | Reject; force ticket + `human_agent` |

## Growth points

- Expand chat-capable connectors when vendor CLIs expose real deny/allow lists.
- Scratch janitor / retention policy.
- Optional secondary session events on `/ws/events`.
- Multi-agent or group sessions remain explicitly out of scope until a later milestone.

## Design-gate review record

This contract locks M09 milestone decisions against current schema (`003`/`011`), context profiles, provider flags, knowledge admin authz, and live WS. Implementation waits for M07 + M08 acceptance. Docs-only delivery for this ticket: spec + plan + philosophy amendment paragraph.
