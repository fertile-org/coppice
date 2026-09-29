# Knowledge — Agent compaction and full-text retrieval

**Status:** Accepted (implemented)  
**Date:** 2026-09-28  
**Milestone context:** Amends [M06 — Knowledge & learning](../../milestones/M06-knowledge-and-learning.md) (shipped) and supersedes the extraction/embedding sections of the [M06 knowledge design](2026-08-03-m06-knowledge-and-learning-design.md). Governance (Inbox, revisions, approval policy, supersession, usage audit) is unchanged.

## Decision summary

M06 ships two pluggable stages: a deterministic keyword `MockExtractionProvider` (the worker hardcodes it; the config field is ignored) and an `EmbeddingProvider` (hash mock or OpenAI-compatible/Ollama) backing pgvector retrieval. The extractor rarely yields useful candidates on real tickets, and embeddings add a second AI stack, a separate service, restart-only TOML config, and a vector dimension coupled to the database column.

**Change:**

1. **Extraction becomes an agent run.** A workspace admin picks one existing agent as the **knowledge compaction agent**. Tickets moved to Done are always queued, and that agent compacts the queue **in periodic batches** (plus a manual **Compact now**), proposing knowledge candidates through a strict result contract. Candidates pass the existing validation and fail-closed policy into the Inbox.
2. **Retrieval becomes Postgres full-text search only.** No embeddings, no fallback, no vector store.
3. **No compaction agent configured → no compaction at all.** Done tickets still accumulate in the queue (cheap, no agent work), so configuring an agent later compacts the backlog. The Knowledge page shows a warning with a CTA to the Agents page.
4. **Running state is visible on the Knowledge page. Success is silent; failure notifies.**

Prior art: Hermes Agent (agent-curated `MEMORY.md` + SQLite FTS5 session search, no embeddings) and Claude Code auto memory (agent-written notes, no vector index). Coppice keeps its human governance layer, which neither has.

## Goals

- Real, useful knowledge candidates from Done tickets using agents the operator already configured.
- One AI configuration surface (agents/connectors), zero extra services.
- Batching to amortize agent startup and give the agent cross-ticket context for deduplication.
- Retrieval that works with no configuration and never fails an agent run.
- Clear UI state: not configured, idle, running, failed.

## Non-goals

- Agents searching knowledge mid-run via a tool (MCP/CLI). Retrieval stays one-shot before the run.
- Per-board compaction agents. One workspace-level agent.
- Backfilling tickets that reached Done before this feature's migration.
- Changing approval, revision, supersession, or usage-audit semantics.

## Removed

| Area | Removed |
|------|---------|
| Server | `EmbeddingProvider`, `MockEmbeddingProvider`, `OpenAiCompatibleEmbeddingProvider`, `ensure_schema_dimension`, `ExtractionProvider`, `MockExtractionProvider` keyword rules, `knowledge_worker` extraction/embedding paths |
| Schema | `knowledge_embeddings` (+ HNSW index), `knowledge_jobs`, `schedule_knowledge_extraction_on_done` trigger, `knowledge_items.extraction_job_id`/`extraction_candidate_index` |
| Config | `knowledge.embedding.*`, `knowledge.extraction.provider` |
| UI | Embedding state/error on knowledge cards |
| Lifecycle | "Approved but not yet embedded" state — approval makes the active revision retrievable immediately |

The `vector` extension may remain installed; nothing depends on it.

## Configuration

### Workspace setting

New single-row table (Coppice has no workspace settings table yet):

```sql
CREATE TABLE workspace_settings (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    knowledge_compaction_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO workspace_settings DEFAULT VALUES;
```

- `NULL` = compaction off.
- Deleting the agent clears the setting (compaction turns off).
- A **disabled** agent keeps the setting but compaction does not run; the UI surfaces this as a warning.
- Only admins may change it (`middleware/admin.rs`).

### TOML (operator tuning, not UI)

```toml
[knowledge.compaction]
interval_secs = 1800           # periodic compaction cadence
batch_max_tickets = 10         # tickets per agent run
batch_max_source_bytes = 200000
max_candidates_per_batch = 20
max_attempts = 3
```

## Queue and batching

### Tables

```sql
CREATE TABLE knowledge_compaction_batches (
    id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    trigger TEXT NOT NULL CHECK (trigger IN ('scheduled', 'manual', 'retry')),
    cycle_id UUID NOT NULL,
    cycle_started_at TIMESTAMPTZ NOT NULL,
    ticket_count INT NOT NULL CHECK (ticket_count > 0),
    candidate_count INT CHECK (candidate_count >= 0),
    summary TEXT,
    error_message TEXT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ
);

CREATE TABLE knowledge_compaction_queue (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    batch_id UUID REFERENCES knowledge_compaction_batches(id) ON DELETE SET NULL,
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0)
);

CREATE TABLE knowledge_compaction_batch_tickets (
    batch_id UUID NOT NULL REFERENCES knowledge_compaction_batches(id) ON DELETE CASCADE,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    PRIMARY KEY (batch_id, ticket_id)
);

CREATE UNIQUE INDEX knowledge_compaction_one_active_batch
    ON knowledge_compaction_batches ((true)) WHERE status IN ('queued', 'running');
```

`knowledge_compaction_batch_tickets` keeps batch membership after queue rows are deleted on success. The batch's run is found through `agent_runs.compaction_batch_id` (no `run_id` column on the batch). Deleting the agent cascades to its batches; the settings pointer is set to null.

### Enqueue

A trigger on `tickets` status change `* → done` always inserts into `knowledge_compaction_queue` (`ON CONFLICT DO NOTHING`), whether or not a compaction agent is configured. Reopening a ticket (`done → *`) deletes its unbatched queue row. Moving it back to Done re-enqueues it. Queueing is not compaction: no agent work happens until an agent is configured.

### Scheduler

A lightweight loop in the existing worker process checks every `knowledge.poll_interval_ms` and starts a **scheduled cycle** at `max(last cycle start, oldest eligible enqueued_at) + interval_secs` (so a lone fresh ticket waits a full interval while an old backlog compacts right away), when all hold:

- the compaction agent is configured and enabled;
- no batch is `queued`/`running` (unique index enforces one at a time);
- eligible unbatched queue rows exist (`attempts < max_attempts`).

A cycle **drains** the queue: it creates a batch from up to `batch_max_tickets` oldest eligible rows (`FOR UPDATE SKIP LOCKED`), trimmed further if their snapshots exceed `batch_max_source_bytes`; when that batch succeeds and eligible rows remain, the next batch starts immediately. The cycle ends when the queue is empty or a batch fails. Batch creation sets `batch_id` and creates the batch plus its agent run in one transaction.

**Compact now** (manual, admins and members) starts a drain cycle immediately, regardless of the interval. It is how operators compact a backlog right after configuring an agent.

### Agent run

Compaction runs are regular `agent_runs` rows so they reuse the job queue, live console, and provider plumbing:

- `agent_runs.compaction_batch_id UUID REFERENCES knowledge_compaction_batches(id)`; the existing ticket-xor-chat check becomes exactly-one-of `ticket_id`, `chat_session_id`, `compaction_batch_id`.
- `job_type = 'compact_knowledge'`, new `context_profile = 'knowledge_compaction'`.
- No repository and no worktree: the working directory is a scratch directory under `WORKTREES_PATH`, like chat sessions without a repo.
- `read_only_tools = true`; connectors that cannot enforce read-only fail closed, consistent with chat.
- Compaction runs never retrieve knowledge into their own context.
- Compaction runs do **not** call `create_for_run_finished` and do not appear in ticket drawers.

### Context (`.agent/context.md`)

- Instructions: reuse litmus and type guidance from `web/src/features/knowledge/curationGuide.ts` (single source of truth, served or duplicated at build), plus Hermes-style save/skip rules: save durable conventions, commands, bug patterns, and rules; skip ticket outcomes, progress logs, and one-off fixes. Empty output is success.
- **Existing knowledge:** titles, ids, and scopes of approved and pending items for the batch's boards (bounded), so the agent can skip duplicates or propose `supersedesItemId`.
- **Per ticket:** id, board id and name, title, description, final acceptance criteria, latest comments and review feedback, and run summaries. Bounded per ticket and per batch.

### Result contract

The agent returns (fenced JSON, same parsing path as other runs):

```json
{
  "status": "done",
  "summary": "Compacted 8 tickets into 3 candidates.",
  "knowledgeCandidates": [
    {
      "type": "test_command",
      "scope": "board",
      "boardId": "…",
      "agentId": null,
      "title": "Use make test-unit while iterating",
      "content": "…",
      "confidence": "high",
      "sourceTicketIds": ["…", "…"],
      "supersedesItemId": null,
      "requiresHumanApproval": false
    }
  ]
}
```

The server validates each candidate. Invalid candidates are dropped and recorded in the batch summary; they never fail the whole batch:

- `sourceTicketIds` is non-empty and every id is in this batch.
- `scope = board` requires `boardId` to equal the board of every source ticket. `scope = workspace` is allowed but always goes to human review. `scope = agent` requires a valid agent.
- `supersedesItemId`, if set, must be an approved item in scope. It creates a supersession **candidate** (Pending), never an automatic supersede.
- Existing domain limits (title/content length, types) and the `policy_decision` fail-closed rules apply unchanged. High-impact types always go to Pending.
- At most `max_candidates_per_batch` are kept.

Provenance: new `knowledge_items.compaction_batch_id` + `compaction_candidate_index` (unique pair, replacing the extraction columns), revision `source_type = 'ticket'`, `source_id` = first source ticket, `source_run_id` = the compaction run, and a `knowledge_item_sources(item_id, ticket_id)` table for all source tickets.

### Completion and failure

- **Success:** batch `succeeded`, `candidate_count` set, its queue rows deleted. No notification. The drain cycle continues if eligible rows remain.
- **Failure** (provider error, timeout, unparseable output, `blocked`): batch `failed` with `error_message`; queue rows get `batch_id = NULL` and `attempts + 1`. Rows reaching `max_attempts` stay in the queue but are excluded from automatic batching until a manual retry resets `attempts`. A `knowledge_compaction_failed` notification (new `notifications.type`, `source_key = knowledge_compaction_failed:{batch_id}`) fans out to workspace users and links to the Knowledge page.
- **Cancel:** allowed from the Knowledge page (reuses run cancellation). Same as failure but without incrementing `attempts` and without a notification.
- A failure ends the current drain cycle. The next scheduled cycle retries naturally (failed tickets are back in the queue with `attempts + 1`), so a persistent failure costs at most `max_attempts` runs per ticket. The failure banner's **Retry** resets `attempts` for failed tickets and starts a `retry` cycle immediately.

## Retrieval — full-text search

```sql
ALTER TABLE knowledge_revisions ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', unaccent_immutable(title)), 'A') ||
        setweight(to_tsvector('simple', unaccent_immutable(content)), 'B')
    ) STORED;
CREATE INDEX knowledge_revisions_search_idx ON knowledge_revisions USING gin (search_vector);
```

- `simple` config plus `unaccent` (immutable wrapper) handles Vietnamese and code identifiers better than `english` stemming.
- **Query building:** normalize the ticket title and description, tokenize, drop stopwords and very short tokens, and keep up to 32 distinct terms (title terms first). Join them with `|` into a `to_tsquery('simple', …)`. Never use `plainto_tsquery`/`websearch_to_tsquery` for ticket text: they AND every term and would almost never match.
- **Ranking:** `ts_rank_cd(search_vector, query)` descending, then revision `created_at` descending, then item id. Same eligibility CTE as today (approved, active revision, not superseded/expired, scope match).
- **Small corpus:** if every eligible item for the board and agent fits `context_budget`, include all of them in deterministic order and skip ranking.
- **Zero matches:** no knowledge section. Never an error; retrieval cannot fail a run.
- `knowledge_usage_logs.similarity` is renamed to `score` (full-text rank, or `0` for include-all).
- **Similar items** (`GET /api/knowledge/:id/similar`, used for duplicate hints on the Knowledge page): the same full-text query built from the item's title and content, excluding itself.

## API

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/settings/knowledge` | `{ compactionAgentId, compactionAgent: {id, name, enabled, connector} \| null }` |
| `PUT` | `/api/settings/knowledge` | Admin; `{ compactionAgentId: uuid \| null }`; agent must exist |
| `GET` | `/api/knowledge/compaction` | Status: `state` (`not_configured` \| `agent_disabled` \| `running` \| `failed` \| `idle`), `configured`, `agent`, `queuedCount`, `blockedCount` (max attempts), `oldestQueuedAt`, `activeBatch`, `lastBatch` (status, counts, summary, error, runId, timestamps), `nextScheduledAt` (idle/failed only), `intervalSecs`, `batchMaxTickets` |
| `POST` | `/api/knowledge/compaction/run` | Compact now: start a drain cycle (409 if a batch is active, nothing queued, or no enabled agent) |
| `POST` | `/api/knowledge/compaction/retry` | Retry failed tickets (resets attempts) |
| `POST` | `/api/knowledge/compaction/cancel` | Cancel the active batch |

The SPA gets updates by invalidating the compaction status query on existing run events over `/ws`, and polls every 5 s while a batch is active as a safety net.

## UI/UX

### Agents page — "Knowledge compaction" card

Placed above the agents table.

```
┌ Knowledge compaction ─────────────────────────────────────────┐
│ Agent that turns Done tickets into knowledge candidates for   │
│ review. Leave empty to turn compaction off.                   │
│                                                               │
│ Compaction agent  [ Reviewer (claude-code)          ▾ ]  Save │
│                                                               │
│ Runs every 30 min in batches of up to 10 tickets, read-only,  │
│ no repo. 12 Done tickets are waiting.                         │
└───────────────────────────────────────────────────────────────┘
```

- The select lists enabled agents plus "None — compaction off". A currently selected disabled agent appears with a "(disabled)" suffix and an amber hint "This agent is disabled; compaction is paused."
- **No agents yet:** the select is disabled, with "Create an agent first" and a **New agent** button that opens the existing create dialog.
- Non-admins see the value read-only.
- Connectors that cannot enforce read-only tools are shown but marked "Cannot run compaction (no read-only mode)" and cannot be saved.
- The configured agent's table row gets a small "Knowledge compactor" badge.
- Anchor `#knowledge-compaction` so the Knowledge page CTA scrolls to and focuses the card.

### Knowledge page — compaction status strip

A single strip under the page header, above the status tabs. Exactly one state shows at a time:

| State | Visual | Content | Actions |
|-------|--------|---------|---------|
| Not configured | Amber warning banner | "Knowledge compaction is off. {n} Done tickets are waiting; choose a compaction agent to turn them into knowledge candidates." | **Choose agent** → `/agents#knowledge-compaction` |
| Agent disabled | Amber warning banner | "Compaction is paused — {agent} is disabled. {n} tickets waiting." | **Open agents** |
| Idle | Muted one-line status | "Compacted by {agent} every {interval} · {n} tickets waiting · last run {relative time}, {k} candidates · next run {relative time}" | **Compact now** (disabled when n = 0) |
| Running | Moss info strip with spinner | "{agent} is compacting {n} tickets… started {relative time}" | **View run** (live console), **Cancel** |
| Last batch failed | Danger banner | "Last compaction failed: {error, truncated}. {n} tickets not compacted." | **Retry**, **View run** |

- When a batch finishes successfully, the strip returns to Idle and the Pending list refetches. There is no toast or notification.
- When it fails, the danger banner appears and a notification lands in the bell (links to `/knowledge`).
- Pending cards created by compaction show "From {k} tickets" with the source tickets listed (each opens the ticket drawer), plus "Proposed by {agent}".
- Embedding state/error is removed from cards.

### Notifications

- New type `knowledge_compaction_failed`: title "Knowledge compaction failed", body "{agent}: {error}", link `/knowledge`.
- Successful and cancelled compaction runs produce no notification.

## Testing

- **Fixtures:** `fixtures/agent-responses/` gains compaction responses (valid multi-candidate, empty, invalid source ticket, malformed JSON). `MockProvider` only.
- **Unit:** candidate validation (scope/board/source checks, supersede checks, policy), tsquery builder (OR join, term cap, unaccent, empty input), include-all budget path.
- **Integration (embedded Postgres):**
  - Done → queued once, with or without an agent configured. Reopen → dequeued. No agent configured → queue grows, no batches created.
  - Scheduled cycle respects `interval_secs`; drain cycle chains batches until the queue is empty; one active batch at a time; Compact now; 409 when a batch is active or no enabled agent.
  - Success → candidates Pending/auto-approved per policy, queue rows cleared, no notification.
  - Failure → batch failed, cycle stops, attempts incremented, `knowledge_compaction_failed` notification once; next scheduled cycle retries; max-attempts exclusion; manual Retry resets attempts; cancel.
  - Agent deleted → setting cleared. Agent disabled → scheduler idle, queue preserved.
  - Full-text retrieval ranking, eligibility filters, zero-match run succeeds without a knowledge section, usage log `score`.
  - Similar endpoint via full-text search.
- **Web:** Agents card states (none, no agents, disabled agent, non-admin), Knowledge strip for all five states, CTA navigation.
- **Smoke:** replace `make e2e-smoke-m06-knowledge` flow: configure compaction agent → move ticket to Done → Compact now → Pending candidate → approve → Full run includes it.

## Migration

1. Create `workspace_settings`, the compaction tables, `knowledge_item_sources`, `agent_runs.compaction_batch_id` + check, the `knowledge_compaction_failed` notification type, and `search_vector` + GIN.
2. Existing items keep revisions and approval state. Mock-extraction provenance (`extraction_job_id`) is dropped, not migrated.
3. Drop the trigger `tickets_schedule_knowledge_extraction`, `knowledge_jobs`, `knowledge_embeddings`, the extraction columns; rename `knowledge_usage_logs.similarity` → `score`.
4. Approved items with an `active_revision_id` stay retrievable. Approved items that never finished embedding (`active_revision_id IS NULL`) get `active_revision_id = current_revision_id`.
5. Startup no longer validates embedding dimension. Legacy `knowledge.embedding.*` and `knowledge.extraction.*` keys are ignored, so old configs still boot.

## Resolved decisions

1. **Backlog:** Done tickets are always queued; configuring an agent later compacts them on the next scheduled cycle or via Compact now. Tickets done before this migration are not queued.
2. **Failure notification audience:** all workspace users, matching existing notification fan-out.

## Open questions

1. **Defaults:** 30-minute interval and 10 tickets per batch are guesses; tune after real usage.
