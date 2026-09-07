# M09 — Agent Chat

## Goal

Add a **first-class, human-owned chat surface** where a human talks to a chosen agent outside a ticket thread — durable sessions, streamed replies, read-oriented tooling, and compact actions that route lasting work back onto the board (create ticket) or Knowledge Inbox (propose knowledge). After M09, exploratory Q&A no longer has to pollute a ticket comment stream, while ticket-bound `@mention` Chat mode (`human_chat`) stays unchanged.

## Product positioning (philosophy amendment)

Product design historically said agents communicate through ticket comments, not hidden chats. M09 **amends** that for **human↔agent** exploration only:

| Channel | Role after M09 |
|---------|----------------|
| Ticket comments + `@mention` Agent/Chat | Official **ticket-bound** work and inter-agent workflow (unchanged) |
| Agent Chat sessions | Official **human↔agent** exploratory conversation — visible in SPA, auditable, not agent-to-agent |

Hard rules:

- Sessions are **human-inspectable** UI + API — never a hidden side channel between agents.
- Agents still do **not** use chat sessions to talk to each other; inter-agent protocol remains ticket comments / mentions.
- Chat is not a second Kanban. Durable execution and status live on **tickets**; durable memory still goes through **M06 Knowledge Inbox** approval.

## Product scope

### Data model

- `chat_sessions` — project-scoped, human-owned; bound agent; optional bound repository; status (`active` / `archived` / `cutoff`); timestamps
- `chat_messages` — ordered turns (`human` \| `agent` \| `system`); body; optional linked `agent_run_id`; action payloads (create-ticket / create-knowledge results)
- Session list + detail APIs; soft archive; hard delete out of scope for v1

### Context profile + run kind

- New context profile: `conversation` (name locked in design gate if a better enum label is needed)
- Distinct from existing profiles:
  - `full` — ticket work / observation
  - `human_agent` — ticket `@mention` Agent mode
  - `human_chat` — ticket `@mention` Chat mode (comment reply only)
- New job type (or run kind): `chat_turn` — one agent reply turn for a session message
- Knowledge retrieval: **off** for `conversation` (same narrow posture as `human_chat`); do not bypass M06 inbox by stuffing retrieval into chat

### Cwd policy

Resolver:

1. If session has a bound registered repo → cwd = that repo’s `local_path` (read-oriented; see write denial)
2. Else → cwd = `$WORKTREES_PATH/chat/{session_id}/` (ephemeral scratch tree created on first turn)

No ticket worktree sharing. Chat must not mutate an in-flight ticket worktree.

### Write-tool denial

- Orchestrator + connector enforce **no write tools** for `conversation` / `chat_turn` (edit, write, apply_patch, git commit/push, etc.)
- Read tools (read, grep, glob, list) allowed within cwd policy
- Violations fail the turn with a clear blocker/error — do not silently no-op

### Streaming

- Prefer **reuse** of existing agent-run live stream (WS/SSE used by M04 / OpenCode live session) keyed by the turn’s `agent_run_id`
- Optional thin session-level event (`chat.message.created`, `chat.turn.finished`) on `/ws/events` for list badges — not a second full token protocol
- Design gate must pick one primary path and document reconnect/replay

### Action contracts (agent result)

Compact, structured actions from a chat turn (server-validated):

| Action | Behavior |
|--------|----------|
| `create_ticket` | Create ticket from chat summary; link `source_chat_session_id`; return ticket id in message metadata |
| `create_knowledge` | Enqueue **pending** Knowledge Inbox item (M06); never auto-approve |
| `cutoff` | Mark session cutoff — no further turns until human reopens/unarchives; preserves history |

No direct status workflow transitions from chat. No silent board moves.

### UI

- New Chat area in SPA (project-scoped session list + transcript)
- Reuse `web/src/opencode-session/` **selectively**: message/part rendering and theme where it fits streamed assistant turns; do **not** fork the full OpenCode TUI session model as product identity
- Composer: pick agent (+ optional repo); send message; show live turn; surface action results (ticket link, inbox item link)

## Out of scope

- Agent↔agent chat sessions
- Replacing ticket `human_chat` mention mode
- Write-enabled coding from chat (use tickets + `human_agent` / Run Agent)
- Auto-approved knowledge from chat
- Multi-agent rooms / group chat
- Mobile-native client
- Scheduled/proactive agent-initiated DMs
- Changing M07 sandbox/secrets/signals or M08 connector CLI beyond what chat cwd/write-denial needs

## Ordering

Milestones stay sequential:

```text
… → M06 → M07 Trust & signals → M08 Connector operator CLI → M09 Agent Chat
```

Do **not** implement M09 before M07 and M08 acceptance. Design-gate docs (spec + plan) may be authored in parallel with M07/M08 implementation so coding can start immediately after M08.

## Dependencies

- M03–M04: runs, live stream, artifacts
- M05: comments/mentions remain the ticket protocol (`human_chat` coexistence)
- M06: Knowledge Inbox for `create_knowledge`
- M07: sandbox/capabilities should already constrain process execution; chat adds profile-level write denial on top
- M08: real connectors available for manual chat dogfood; CI stays on `MockProvider`

## Architecture notes

### Design gate (required before coding)

1. Spec under `docs/superpowers/specs/` — API, cwd resolver, `conversation` vs `human_chat`/`full`, write-denial matrix, streaming choice, action schemas, UI reuse map, philosophy note
2. Plan under `docs/superpowers/plans/` — sequenced tasks + test matrix
3. Explicit non-goals and regression cases for M06 inbox + existing mention Chat mode

### Suggested server modules

```text
server/src/
  domain/
    chat_session.rs
    chat_message.rs
  services/
    chat_service.rs
    chat_cwd.rs           # repo vs WORKTREES_PATH/chat/{id}
  api/
    chat.rs
  # reuse run pipeline for chat_turn + conversation profile
```

### Suggested tables

```text
chat_sessions
chat_messages
```

### API sketch (lock in design gate)

```text
GET    /api/chat/sessions
POST   /api/chat/sessions
GET    /api/chat/sessions/:id
PATCH  /api/chat/sessions/:id          # archive / reopen
POST   /api/chat/sessions/:id/messages # human turn → enqueue chat_turn
GET    /api/chat/sessions/:id/messages
POST   /api/chat/sessions/:id/cutoff
```

Live tokens: reuse `GET /ws/agent-runs/:runId/live` for the turn run.

## Testing strategy

### Unit

- Cwd resolver: bound repo vs `$WORKTREES_PATH/chat/{session_id}/`
- Write-tool denial matrix for `conversation`
- Action parsers: `create_ticket`, `create_knowledge` (pending only), `cutoff`
- Context builder: `conversation` excludes knowledge retrieval and full ticket thread

### Integration

- Create session → post message → mock `chat_turn` → message persisted + run linked
- `create_ticket` action → ticket exists with source session link
- `create_knowledge` → inbox pending; not approved
- Ticket comment `mentionMode=chat` still uses `human_chat` / `respond_to_mention` only (no chat_sessions row)

### Smoke (post-implement)

- `make e2e-smoke-m09` (or extend an existing smoke) — session create, one mock turn, cutoff
- Default compose remains mock provider

## Acceptance criteria

- [ ] Milestone + approved design spec + implementation plan exist (API, cwd, profile, actions, UI reuse)
- [ ] `chat_sessions` / `chat_messages` (or equivalent) persisted; human can list/open history in SPA
- [ ] `conversation` profile + `chat_turn` run kind distinct from `human_chat` / `full`
- [ ] Cwd resolver implements repo-bound vs `$WORKTREES_PATH/chat/{session_id}/`
- [ ] Write tools denied for chat turns (orchestrator + connector); reads allowed per policy
- [ ] Streaming reuses run live channel (documented); session list updates via events or poll
- [ ] Actions: create-ticket, create-knowledge (inbox pending), cutoff — server-validated
- [ ] No regression: ticket `@mention` Chat mode still `human_chat`; M06 inbox still requires human approve
- [ ] CI/mock path green; no real CLI required in automated tests

## Related docs

- [M06 — Knowledge & learning](./M06-knowledge-and-learning.md)
- [M07 — Trust & signals](./M07-trust-and-signals.md)
- [M08 — Connector operator CLI](./M08-connector-operator-cli.md)
- [Human @mention agent runs](../superpowers/specs/2026-06-14-human-mention-agent-runs-design.md) — `human_chat` coexistence
- [Product design](../philosophy/final_agent_workspace_product_design.md) — §3.2 / §9.1 amended by this milestone for human↔agent chat only
