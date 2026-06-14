# Human @Mention Agent Runs — Design Spec

**Date:** 2026-06-14  
**Status:** Draft (pending review)  
**Product:** Coppice — agent workspace on tickets

**Depends on:** M05 workflow/mentions, M03 runs/job worker, M04 live console  
**Builds on:** `POST /api/tickets/:id/comments` mention parsing, `work_on_ticket`, `respond_to_mention`

## Purpose

Let humans `@mention` an agent in a ticket comment and have that agent run immediately — with **Agent** mode (execute in the shared worktree, full run/live-console/history) or **Chat** mode (reply in thread only) — without the two-step “comment, then Run Agent” flow.

The execute path must be **the same run pipeline** as **Run Agent** (worktree, branch, live console, runs tab). The only intentional difference is **how context is assembled** so the human’s comment is highest priority.

## Problem (today)

| Path | UX | Context |
|------|-----|---------|
| **Run Agent** | Manual button after comment | Full ticket description + ~4k comment thread + soul; human note buried in thread |
| **@mention in comment** | Hidden / no UI affordance | Starts `respond_to_mention` only; same heavy context; tuned for clarification not execution |

Humans want: `@tech_lead please also fix the doc…` → agent runs on ticket worktree with **human instruction first**, optional on-demand fetch for the rest.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Execute vs chat | **Both**, user-selectable per comment (like OpenCode/Cursor chat vs agent mode) |
| Default mode | **Agent** (execute when `@mention` present) |
| UI control | `<select>` on comment composer: **Agent** \| **Chat** |
| Execute pipeline | Same as Run Agent: `work_on_ticket`, shared worktree, new run, live console, runs history |
| Chat pipeline | `respond_to_mention`, comment-only reply |
| Target agent | **Mentioned** agent (not required to be assignee) |
| Routing | Explicit mode select (not keyword/LLM inference in v1) |
| Workflow on human Agent runs | **No automatic status transition** on `done` |
| On-demand context (v1) | Static JSON snapshots under `.agent/` readable by agent tools |
| Multi-mention | **One agent per comment** in v1 |

---

## Architecture overview

```text
Human posts comment (+ optional @agent, mentionMode)
  → CommentService.create
  → MentionService.create_mentions (link comment_id)
  → if @agent + mentionMode=agent:
       RunService.start_run_for_agent(agent, work_on_ticket, profile=human_agent, trigger_comment_id)
     if @agent + mentionMode=chat:
       RunService.start_run_for_agent(agent, respond_to_mention, profile=human_chat, trigger_comment_id)
  → job_worker builds context from profile
  → same provider / live stream / orchestrator path
```

**Approach:** Context **profiles** on existing job types (not a new job type).

| Profile | Job type | Trigger |
|---------|----------|---------|
| `full` | `work_on_ticket` | Run Agent button, assignee auto-start |
| `human_agent` | `work_on_ticket` | Comment + @mention + Agent mode |
| `human_chat` | `respond_to_mention` | Comment + @mention + Chat mode |

---

## UX — comment composer

### Mode selector

- Control: `<select>` adjacent to comment textarea
- Options: **Agent** (default), **Chat**
- Labels/tooltips:
  - **Agent** — “Run mentioned agent in ticket worktree to execute your request”
  - **Chat** — “Ask mentioned agent; reply in comments only”

### @mention autocomplete

- Typing `@` opens picker of project agent keys (`tech_lead`, `backend_engineer`, name slug, preset key)
- Insert `@tech_lead` (no spaces in key)
- v1: validate exactly **one** mention when mode triggers a run; show error if 0 or >1

### Post-submit feedback

- Toast: “Started run for Tech Lead Agent” with action to open Live tab
- Optional: auto-focus Live tab when run starts (configurable / nice-to-have)

### Run Agent button

- Unchanged: runs **current assignee** with profile `full`
- Does not require a comment

---

## Context profiles

### Shared: Human request block (human_agent + human_chat)

Always first section in `.agent/context.md`:

```markdown
# Human request (read this first)

**From:** Human
**Posted:** <ISO8601>
**Mode:** Agent | Chat

> <full comment body, not truncated>

This instruction overrides ticket description and thread summaries when they conflict.
```

For `human_agent`, add:

```markdown
Execute in the ticket worktree unless the request is purely informational (then reply in your result summary only).
```

### human_agent — minimal snapshot

Include:

- Human request block (above)
- **Ticket snapshot:** title, status, substatus, assignee name/key, ticket id
- **Repository:** worktree path, branch, remote (same as today)
- **Agent role:** soul, skills, responsibilities
- **Expected output contract:** standard done/blocked/continued
- **Platform rules:** scoped to human-directed work (commit changes, omit assignTo, no workflow assumptions)

Omit from inline context:

- Full ticket description
- Full comment thread
- Acceptance criteria body

### human_chat — conversational

- Human request block
- Short thread excerpt: last 3 non-system comments, ~800 chars total
- Ticket snapshot (title, status only)
- Agent role + reply-oriented contract (concise answer; no worktree execution emphasis)

### full — unchanged

Current behavior: description + `## Ticket thread` (up to `TICKET_THREAD_MAX`) + verification/git rules.

---

## On-demand context (v1)

Write at run start under `.agent/` (gitignored, excluded from auto-commit):

| File | Contents |
|------|----------|
| `ticket.json` | id, title, status, substatus, description, acceptance_criteria, assignee_agent_id/key |
| `comments.json` | All comments: id, author, intent, body, created_at (newest first) |
| `runs.json` | Last 10 runs: id, agent, job_type, status, started_at, ended_at |

Context instructs:

```markdown
## On-demand ticket data

If you need full description, history, or past runs, read:
- `.agent/ticket.json`
- `.agent/comments.json`
- `.agent/runs.json`

Do not load these unless necessary for the human request.
```

**v2 (out of scope):** live refresh mid-run via MCP/HTTP tool.

---

## Run metadata

Extend `agent_runs` (migration):

```sql
ALTER TABLE agent_runs
  ADD COLUMN context_profile TEXT NOT NULL DEFAULT 'full',
  ADD COLUMN trigger_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL;
```

Valid `context_profile`: `full`, `human_agent`, `human_chat`.

Worker reads these when building `ContextInput` and writing `.agent/*`.

---

## Workflow & orchestration

### human_agent (`work_on_ticket`)

- Same worktree sync, git footer, live console as normal `work_on_ticket`
- **On succeeded `done`:** post agent comment + git footer; **do not** apply workflow status gates (`resolve_transition` no-op for status when `context_profile == human_agent`)
- Assignee unchanged unless human explicitly assigns separately
- Clarification resume (`resume_agent_id` on mentions) **not** used for human Agent mode in v1

### human_chat (`respond_to_mention`)

- Existing behavior: no status change on succeed
- Context uses `human_chat` profile instead of full thread
- Comment intent: `clarification_answer` or new `human_chat_reply` (prefer reuse `clarification_answer` in v1)

### Blocked runs

- Agent blocked → existing blocked path; mention handling unchanged

---

## API

### POST `/api/tickets/:id/comments`

Request body extension:

```json
{
  "body": "@tech_lead please fix the doc…",
  "mentionMode": "agent",
  "attachmentIds": []
}
```

`mentionMode`: `"agent"` | `"chat"` (default `"agent"` when omitted).

Server flow:

1. Create comment (human author)
2. `parse_mention_keys(body)` → agent keys
3. If keys empty → return comment only
4. If keys.len() > 1 → `400` with clear error (v1)
5. If ticket has no `repo_id` and mode is `agent` → `400` (“repo required to run agent”)
6. Create `ticket_mentions` (status pending, no resume_agent)
7. Start run:
   - `agent` → `start_run_for_agent(id, work_on_ticket)` with profile + trigger_comment_id
   - `chat` → `start_run_for_agent(id, respond_to_mention)` with profile + trigger_comment_id
8. Respect `workflow.auto_start_runs`; **human Agent/Chat mentions always enqueue** (explicit human intent — recommend starting run even if global auto_start is false; document in config)

**Decision:** Human `@mention` with Agent/Chat mode **always starts run** when repo present, independent of `auto_start_runs`. Rationale: posting in Agent mode is an explicit run request.

Response: include optional `startedRuns: [{ runId, agentId, agentKey }]`.

---

## Web changes

| File / area | Change |
|-------------|--------|
| `TicketCommentsTab` | Mode `<select>`, @ autocomplete, post `mentionMode` |
| `useTicket` / `postComment` | Pass `mentionMode`; prepend comment; handle `startedRuns` |
| `TicketDrawer` | On started run from comment → invalidate runs, optional Live tab focus |
| Types | `MentionMode = 'agent' \| 'chat'` |

---

## Server modules

| Module | Change |
|--------|--------|
| `context_builder.rs` | `ContextProfile` enum; `human_request` section; profile-specific omission rules |
| `job_worker.rs` | Load profile + trigger comment; write `.agent/ticket.json` etc.; skip full thread for human profiles |
| `run_service.rs` | `start_run_for_agent(..., context_profile, trigger_comment_id)` |
| `workflow_service.rs` | Skip gate transitions when profile is `human_agent` |
| `comments.rs` | Accept `mentionMode`; start runs with metadata |
| `mention_service.rs` | Unchanged parse logic |

---

## Testing

### Unit

- Context builder: `human_agent` omits description/thread; human block first
- Context builder: `human_chat` includes short excerpt only
- Workflow: `human_agent` done does not change status
- Mention parse + single-agent validation

### Integration

- Post comment `@pm …` + `mentionMode=chat` → `respond_to_mention` run, status unchanged
- Post comment `@backend_engineer …` + `mentionMode=agent` → `work_on_ticket`, worktree path set, profile `human_agent`, status unchanged after done
- `.agent/ticket.json` exists in worktree during run

### Web

- Mode select defaults to Agent
- Post with mention shows toast / run in list

---

## Out of scope (v1)

- Multiple @mentions in one comment
- LLM-based execute vs chat inference
- Live mid-run context refresh tool
- “Apply workflow on completion” toggle for human Agent runs
- Changing assignee on mention-run
- @mention in agent-authored comments triggering new human-style runs

---

## Migration & rollout

1. DB migration for `context_profile`, `trigger_comment_id`
2. Server: profiles + API + workflow skip
3. Web: composer UX
4. Docs: update M05 mention section + user-facing help string in UI

---

## Open questions (resolved)

| Question | Resolution |
|----------|------------|
| Same worktree as Run Agent? | Yes |
| New job type? | No — profiles on existing types |
| auto_start_runs gate? | Human mention run always starts (explicit intent) |
| Status on human agent done? | No automatic transition |

---

## Spec self-review

- [x] No TBD placeholders in core flows
- [x] Consistent with M05 mention table and job types
- [x] Scoped to single implementation plan (one milestone slice)
- [x] `human_agent` workflow skip explicitly documented to avoid accidental In QA moves on doc-fix requests
- [x] v1 limits (one mention, static JSON fetch) stated
