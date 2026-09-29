# Knowledge Agent Compaction and Full-Text Retrieval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the mock keyword extractor and embedding pipeline with (1) periodic, batched knowledge compaction performed by an admin-selected agent and (2) Postgres full-text retrieval, plus the Agents/Knowledge UI for configuration and status.

**Architecture:** Done transitions always enqueue tickets. A scheduler in the worker process runs drain cycles every `interval_secs` (or on Compact now) when a compaction agent is configured and enabled, creating one `knowledge_compaction_batches` row plus a regular `agent_runs` row (`job_type = compact_knowledge`, read-only, no repo) at a time. The run returns `knowledgeCandidates`, which the server validates and routes through the existing fail-closed policy. Retrieval ranks eligible revisions with `ts_rank_cd` over a generated `tsvector`, or includes everything when it fits the budget. Embeddings, `knowledge_jobs`, and the extraction trigger are removed.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL 16 (`unaccent`), Tokio, React 19, TypeScript, TanStack Query, Zod, Vitest, Docker Compose.

**Design:** `docs/superpowers/specs/2026-09-28-knowledge-agent-compaction-fts-design.md`

**Verification rule:** iterate with `make test-unit`, targeted `cargo test -p coppice-server --features embedded-test-db <filter>`, and `make web-test`. Run `make test` + `cargo clippy --workspace -- -D warnings` only in the final task, then `make clean`.

---

### Task 1: Migration and configuration

**Files:**
- Create: `server/migrations/027_knowledge_agent_compaction_fts.sql`
- Modify: `config/src/lib.rs`
- Modify: `server/src/config/mod.rs`
- Modify: `config.example.toml`, `deploy/config/default.toml`, `deploy/docker-compose.yml`
- Modify: `server/src/db/pool.rs`, `server/src/db/test_embed.rs`, `server/tests/common/mod.rs`

- [ ] **Step 1: Write failing config tests**

In `config/src/lib.rs` tests: `KnowledgeConfig::default().compaction` has `interval_secs = 1800`, `batch_max_tickets = 10`, `batch_max_source_bytes = 200_000`, `max_candidates_per_batch = 20`, `max_attempts = 3`; `validate()` rejects zero values. Loading a TOML containing legacy `[knowledge.embedding]` / `knowledge.extraction.provider` succeeds (keys ignored, warning logged).

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p coppice-config knowledge -- --nocapture`  
Expected: FAIL — `compaction` field missing.

- [ ] **Step 3: Update configuration types**

Add `KnowledgeCompactionConfig`. Remove `EmbeddingConfig` and `ExtractionConfig` from `KnowledgeConfig`; accept and ignore the legacy tables via `#[serde(default)] legacy_embedding: Option<toml::Value>` (or `deny_unknown_fields` off) and emit a `tracing::warn!` at startup when present. Update example/default TOML and remove embedding env vars from `deploy/docker-compose.yml`.

- [ ] **Step 4: Write the migration**

In one file, in this order:
1. `CREATE EXTENSION IF NOT EXISTS unaccent;` plus an `IMMUTABLE` wrapper `unaccent_immutable(text)` (`SELECT public.unaccent('public.unaccent', $1)`).
2. `workspace_settings` single-row table + seed row.
3. `knowledge_compaction_batches`, `knowledge_compaction_queue`, `knowledge_compaction_batch_tickets`, one-active-batch unique index, queue index on `(batch_id, attempts, enqueued_at)`.
4. `agent_runs.compaction_batch_id`; replace `agent_runs_ticket_xor_chat_check` with an exactly-one-of check across `ticket_id`, `chat_session_id`, `compaction_batch_id`; unique active index on `compaction_batch_id`; extend `agent_runs_context_profile_check` with `knowledge_compaction`.
5. `knowledge_items.compaction_batch_id` + `compaction_candidate_index` (paired check, unique pair); `knowledge_item_sources(item_id, ticket_id)` PK.
6. `knowledge_revisions.search_vector` generated column + GIN index.
7. Extend `notifications_type_check` with `knowledge_compaction_failed`; make `notifications.ticket_id` usage optional (already nullable).
8. Backfill: `UPDATE knowledge_items SET active_revision_id = current_revision_id WHERE status = 'approved' AND active_revision_id IS NULL`.
9. Drop trigger `tickets_schedule_knowledge_extraction` and its function; create `enqueue_knowledge_compaction_on_status()` trigger (`* → done` inserts `ON CONFLICT DO NOTHING`; `done → *` deletes rows with `batch_id IS NULL`).
10. Drop `knowledge_items_extraction_job_fk`, extraction columns, `knowledge_jobs`, `knowledge_embeddings`; rename `knowledge_usage_logs.similarity` → `score`.

- [ ] **Step 5: Remove embedding dimension startup logic and update test reset**

Delete `ensure_schema_dimension`/`validate_schema_dimension` calls from `server/src/db/pool.rs` and `test_embed.rs`. Update `truncate_test_workspace` in `server/tests/common/mod.rs`: remove `knowledge_jobs`/`knowledge_embeddings`, add `knowledge_item_sources`, `knowledge_compaction_batch_tickets`, `knowledge_compaction_queue`, `knowledge_compaction_batches` (children first), and reset `workspace_settings` to `NULL` agent rather than truncating it.

- [ ] **Step 6: Run migration-focused tests**

Run: `cargo test -p coppice-config knowledge && cargo test -p coppice-server --features embedded-test-db db:: -- --nocapture`  
Expected: PASS (other knowledge tests will fail until later tasks; do not fix them here beyond compiling).

- [ ] **Step 7: Commit**

`git commit -m "feat(knowledge): compaction schema and config, drop embeddings"`

### Task 2: Remove embedding and extraction code; full-text retrieval

**Files:**
- Delete: `server/src/knowledge/embedder.rs`, `mock_embedder.rs`, `openai_embedder.rs`, `server/src/knowledge/extractor.rs` (move `policy_decision` + `fold_reuse_hint` into `server/src/knowledge/policy.rs`)
- Delete: `server/src/workers/knowledge_worker.rs`, `server/src/services/knowledge_job_service.rs`
- Create: `server/src/knowledge/fts.rs`
- Modify: `server/src/knowledge/mod.rs`, `server/src/knowledge/retrieval.rs`
- Modify: `server/src/services/knowledge_service.rs` (approve/edit/supersede activate immediately; `find_similar` via FTS)
- Modify: `server/src/workers/job_worker.rs` (Full-run retrieval), `server/src/api/knowledge.rs` (similar endpoint), `server/src/main.rs` / worker spawn sites
- Modify: `server/src/services/run_service.rs`, `run_orchestrator.rs`, `split_service.rs`, `agent_service.rs` (remove `knowledge_jobs`/embedding references)

- [ ] **Step 1: Write failing unit tests for the query builder** (`fts.rs`)

- Joins terms with ` | `, lowercases, strips accents is left to SQL (`unaccent_immutable` applied in SQL, not Rust).
- Drops tokens shorter than 3 chars and a small stopword list (English + common Vietnamese function words), dedupes, caps at 32 terms with title terms first.
- Escapes/strips tsquery operators (`& | ! ( ) : * < > '`) so user text can't inject syntax.
- Empty input → `None`.

- [ ] **Step 2: Implement `build_tsquery(title, body) -> Option<String>`** and make tests pass (`make test-unit`).

- [ ] **Step 3: Rewrite retrieval**

`retrieve(pool, board_id, agent_id, query_text, retrieval_cfg, budget)`:
1. Load the eligible set with the existing `MATERIALIZED` CTE (drop the embedding join).
2. If the rendered eligible set fits `context_budget`, return all ordered by `knowledge_type, created_at DESC, item_id` with `score = 0`.
3. Otherwise, if `build_tsquery` returns `Some(q)`, rank with `ts_rank_cd(search_vector, to_tsquery('simple', unaccent_immutable($q)))`, filter `search_vector @@ query`, order by score DESC, revision `created_at` DESC, item id ASC, limit `top_k`.
4. Zero matches → empty vec. No error path depends on external services.

Update `has_eligible` accordingly. In `job_worker.rs`, remove `embedding_provider` usage and pass ticket title + description as `query_text`. Usage logs write `score`.

- [ ] **Step 4: Activate on approval**

In `knowledge_service.rs`, approve/edit-of-approved/supersede-activation set `active_revision_id = current_revision_id` in the same transaction (the old embedding-ready gate is gone). Supersession sets the original's `superseded_by` in that transaction.

- [ ] **Step 5: Similar endpoint via FTS**

`find_similar(item_id, limit)` builds a query from the item's current title + content and ranks other non-rejected items; response field `similarity` becomes `score` (update response struct).

- [ ] **Step 6: Delete dead code and fix compile**

Remove modules/workers listed above and their spawns. `cargo build -p coppice-server` must be clean; `cargo clippy -p coppice-server -- -D warnings`.

- [ ] **Step 7: Update integration tests**

In `server/tests/integration_knowledge.rs`: delete embedding/extraction-job tests (dimension, embed failure preserves revision, extract-on-done job); convert retrieval tests to FTS (matching term ranks first, eligibility filters, zero-match Full run succeeds with no knowledge section, include-all path when under budget); approve → immediately retrievable; similar endpoint.

Run: `cargo test -p coppice-server --features embedded-test-db integration_knowledge`  
Expected: PASS.

- [ ] **Step 8: Commit**

`git commit -m "feat(knowledge): full-text retrieval; remove embeddings and mock extractor"`

### Task 3: Workspace settings API

**Files:**
- Create: `server/src/services/workspace_settings_service.rs`, `server/src/api/settings.rs`
- Modify: `server/src/services/mod.rs`, `server/src/api/mod.rs`
- Test: `server/tests/integration_workspace_settings.rs`

- [ ] **Step 1: Write failing integration tests**

- `GET /api/settings/knowledge` → `{ compactionAgentId: null, compactionAgent: null }` by default.
- Admin `PUT` with a valid agent → persisted; response includes `{ id, name, enabled, health, connector }`.
- `PUT` with unknown agent → 400; non-admin → 403; missing CSRF → 403.
- `PUT` with an agent whose connector cannot enforce read-only tools → 400 (`compaction requires a read-only capable connector`).
- `PUT { compactionAgentId: null }` → off.
- Deleting the agent → setting reads `null`.

- [ ] **Step 2: Implement service + thin handlers**

Service owns validation (agent exists, connector read-only capability via the connector registry — the same check chat uses in `refuse_unsupported_read_only`). Handlers only parse/authorize. `PUT` behind admin middleware.

- [ ] **Step 3: Run tests**

Run: `cargo test -p coppice-server --features embedded-test-db integration_workspace_settings`  
Expected: PASS.

- [ ] **Step 4: Commit**

`git commit -m "feat(settings): knowledge compaction agent setting"`

### Task 4: Compaction queue, batches, and scheduler

**Files:**
- Create: `server/src/domain/knowledge_compaction.rs` (batch status/trigger enums)
- Create: `server/src/services/knowledge_compaction_service.rs`
- Create: `server/src/workers/knowledge_compaction_scheduler.rs`
- Modify: `server/src/services/run_service.rs` (create run with `compaction_batch_id`)
- Modify: `server/src/main.rs` (spawn scheduler)
- Test: `server/tests/integration_knowledge_compaction.rs`

- [ ] **Step 1: Write failing integration tests (queue + scheduling only; runs not executed yet)**

- Done → one queue row with or without configured agent; repeated Done updates → still one; reopen → removed; batched rows are not removed on reopen.
- `tick(now)` with no agent / disabled agent → no batch.
- `tick` before `interval_secs` since last scheduled cycle → no batch; after → one batch with ≤ `batch_max_tickets` oldest eligible tickets, `trigger = scheduled`, queue rows get `batch_id`, `knowledge_compaction_batch_tickets` populated, one `agent_runs` row (`job_type = compact_knowledge`, `context_profile = knowledge_compaction`, `ticket_id` and `chat_session_id` NULL) and its `agent_jobs` row.
- Second `tick` while a batch is active → no new batch (and the unique index rejects a racing insert).
- Byte trimming: tickets whose snapshots exceed `batch_max_source_bytes` are split across batches; a single oversized ticket is truncated, never skipped.
- Rows with `attempts >= max_attempts` are excluded.
- `start_manual(user)` → batch with `trigger = manual` regardless of interval; 409-equivalent errors when active batch / empty queue / no enabled agent.

- [ ] **Step 2: Implement the service**

`KnowledgeCompactionService`:
- `status()` for the API (configured, agent, queued/blocked counts, active batch, last batch, `next_scheduled_at`).
- `create_batch(trigger, created_by)`: one transaction — lock eligible rows `FOR UPDATE SKIP LOCKED` ordered by `enqueued_at`, apply count/byte limits, insert batch + membership, set `batch_id`, create run + job via `RunService`.
- `start_manual`, `retry` (reset `attempts` for rows of the last failed batch and rows at max attempts, then `create_batch(retry)`), `cancel` (delegates to run cancellation).
- `last_scheduled_cycle_at` is derived from the newest `scheduled` batch `created_at` (no extra table).

- [ ] **Step 3: Implement the scheduler loop**

Every `knowledge.poll_interval_ms`: if no active batch and the interval elapsed (or the previous batch in the current cycle succeeded and eligible rows remain), call `create_batch`. Drain continuation: after a batch succeeds, the completion handler (Task 5) calls `create_batch` with the same trigger when eligible rows remain.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server --features embedded-test-db integration_knowledge_compaction`  
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(knowledge): compaction queue, batches, scheduler"`

### Task 5: Compaction run execution, result contract, and notifications

**Files:**
- Create: `server/src/knowledge/compaction_context.rs` (build `.agent/context.md` input)
- Create: `server/src/knowledge/candidates.rs` (parse + validate `knowledgeCandidates`)
- Modify: `server/src/providers/mod.rs` (`AgentRunResult::Done` gains `#[serde(default, rename = "knowledgeCandidates")] knowledge_candidates: Vec<KnowledgeCandidateSpec>`)
- Modify: `server/src/services/context_builder.rs` (render `knowledge_compaction` profile)
- Modify: `server/src/workers/job_worker.rs` (dispatch `compact_knowledge` like `chat_turn`: scratch cwd, `read_only_tools = true`, no retrieval, no ticket side effects)
- Modify: `server/src/services/knowledge_service.rs` (`create_from_compaction`)
- Modify: `server/src/services/notification_service.rs` (+ `NotificationType::KnowledgeCompactionFailed`, `create_for_compaction_failed`)
- Modify: `server/src/services/run_orchestrator.rs` (skip `create_for_run_finished` for compaction runs)
- Create fixtures: `fixtures/agent-responses/<test-agent-key>/compact_knowledge.json` (valid, 2 candidates), plus override fixtures `compact_knowledge_empty.json`, `compact_knowledge_invalid_source.json`, `compact_knowledge_malformed.json`
- Test: extend `server/tests/integration_knowledge_compaction.rs`

- [ ] **Step 1: Unit tests for candidate validation** (`candidates.rs`)

- Rejects empty `sourceTicketIds`, ids outside the batch, `scope = board` with a board that differs from any source ticket's board, unknown type, out-of-bounds title/content.
- `scope = workspace` → forced human review. `supersedesItemId` must reference an approved in-scope item → becomes a Pending supersession candidate.
- Caps at `max_candidates_per_batch`; invalid candidates are returned as drop reasons, not errors.
- `policy_decision` applied unchanged (high-impact → Pending, auto-save only via allowlist + high confidence + no approval request).

- [ ] **Step 2: Unit tests for compaction context**

Snapshot includes instructions (reuse litmus + save/skip rules), existing knowledge titles/ids/scopes for batch boards (bounded), per-ticket id/board/title/description/acceptance criteria/latest comments/review feedback/run summaries, and the output schema. Per-ticket and total byte caps enforced.

- [ ] **Step 3: Implement run execution path**

In `job_worker.rs`, `compact_knowledge` runs: mark batch `running` + `started_at`; build context; invoke provider with `read_only_tools = true`; on `Done`, validate candidates and call `KnowledgeService::create_from_compaction` in one transaction (items, revisions with `source_type = ticket`, `source_run_id`, `knowledge_item_sources`, `compaction_batch_id`/index — idempotent on the unique pair); mark batch `succeeded` with `candidate_count`; delete queue rows; continue the drain cycle.

On provider error / `Blocked` / `Continued` / unparseable output: batch `failed` + `error_message`; queue rows `batch_id = NULL`, `attempts + 1`; `create_for_compaction_failed(batch_id)` (fan-out to all users, `source_key = knowledge_compaction_failed:{batch_id}`, idempotent). On cancel: batch `failed` with `error_message = 'cancelled'`, no attempt increment, no notification.

- [ ] **Step 4: Integration tests**

- Mock fixture success → 2 Pending items (or one auto-approved per policy config), sources recorded, queue emptied, batch `succeeded`, **no** notification rows.
- Drain: 25 queued with `batch_max_tickets = 10` → three sequential batches, all succeeded.
- Empty fixture → succeeded, `candidate_count = 0`.
- Invalid source fixture → succeeded, candidate dropped, reason in batch summary.
- Malformed fixture → failed, attempts = 1, exactly one `knowledge_compaction_failed` per user, cycle stops.
- Retry after failure resets attempts and succeeds with the valid fixture.
- Compaction runs never create `agent_run_finished` notifications and never write `knowledge_usage_logs`.
- Agent with non-read-only connector configured before a connector change → run fails closed with a clear error.

Run: `cargo test -p coppice-server --features embedded-test-db integration_knowledge_compaction`  
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(knowledge): agent compaction runs, candidate contract, failure notifications"`

### Task 6: Compaction API

**Files:**
- Modify: `server/src/api/knowledge.rs`
- Test: `server/tests/integration_knowledge_compaction.rs`

- [ ] **Step 1: Failing API tests**

- `GET /api/knowledge/compaction` for each state: not configured (with `queuedCount`), disabled agent, idle (`nextScheduledAt`), running (`activeBatch.runId`), last failed (`lastBatch.errorMessage`).
- `POST /run` → 202 with batch; 409 when active, empty, or no enabled agent.
- `POST /retry` → 202; 409 when nothing to retry.
- `POST /cancel` → 202; 409 when idle.
- All mutations require CSRF.

- [ ] **Step 2: Implement thin handlers over `KnowledgeCompactionService`**

- [ ] **Step 3: Run tests, commit**

`git commit -m "feat(api): knowledge compaction status and controls"`

### Task 7: Web — schemas, hooks, Agents page card

**Files:**
- Modify: `web/src/lib/schemas/knowledge.ts` (compaction status schema; remove embedding fields; `similarity` → `score`)
- Create: `web/src/lib/schemas/settings.ts`
- Create: `web/src/features/settings/useKnowledgeSettings.ts`
- Modify: `web/src/features/knowledge/useKnowledge.ts` (`useCompactionStatus` with `refetchInterval: 5000` while `activeBatch`, invalidated on run events over `/ws`; `useCompactNow`, `useRetryCompaction`, `useCancelCompaction`)
- Create: `web/src/features/agents/KnowledgeCompactionCard.tsx` + test
- Modify: `web/src/features/agents/AgentsPage.tsx` (render card above table, `id="knowledge-compaction"`, "Knowledge compactor" badge in `AgentRow`)

- [ ] **Step 1: Failing component tests** (`KnowledgeCompactionCard.test.tsx`)

- No agents → select disabled, "Create an agent first", **New agent** opens create dialog.
- Agents exist, none selected → "None — compaction off" selected; saving an agent calls `PUT`.
- Selected disabled agent → "(disabled)" suffix + amber paused hint.
- Non-read-only connector agent → option disabled with reason.
- Non-admin → read-only value, no Save.
- Shows cadence/batch copy and waiting ticket count.
- URL hash `#knowledge-compaction` scrolls into view and focuses the select.

- [ ] **Step 2: Implement** following `docs/web/DESIGN.md` tokens (card: `rounded-xl border border-border bg-surface-raised shadow-card`, amber hint `bg-amber-100 text-amber-900`).

- [ ] **Step 3: Run** `make web-test` → PASS. **Commit** `git commit -m "feat(web): knowledge compaction agent setting on Agents page"`

### Task 8: Web — Knowledge page status strip and card changes

**Files:**
- Create: `web/src/features/knowledge/CompactionStatusStrip.tsx` + test
- Modify: `web/src/features/knowledge/KnowledgePage.tsx` (render strip under header; remove embedding state/error; show "From {k} tickets" source list and "Proposed by {agent}" for compaction items)
- Modify: `web/src/features/knowledge/KnowledgePage.test.tsx`
- Modify: `web/src/features/notifications/NotificationBell.tsx` (+ test) for `knowledge_compaction_failed` → link `/knowledge`

- [ ] **Step 1: Failing tests** — one per state:

| State | Assert |
|-------|--------|
| Not configured | Amber banner with waiting count; **Choose agent** navigates to `/agents#knowledge-compaction` |
| Agent disabled | Amber paused banner; **Open agents** |
| Idle | Muted line with agent, interval, waiting count, last run, next run; **Compact now** disabled at 0 queued, calls POST otherwise |
| Running | Spinner, agent, ticket count, started time; **View run** opens live console; **Cancel** calls POST |
| Failed | Danger banner with truncated error and ticket count; **Retry**, **View run** |

Also: success transition (running → idle) refetches Pending list and shows no toast; knowledge cards no longer render embedding status; notification bell renders the new type.

- [ ] **Step 2: Implement**, then `make web-test` → PASS.

- [ ] **Step 3: Commit** `git commit -m "feat(web): knowledge compaction status on Knowledge page"`

### Task 9: Smoke, docs, final verification

**Files:**
- Modify: `e2e/smoke/m06-knowledge.mjs` (+ `Makefile` target description)
- Modify: `docs/architecture.md`, `docs/testing.md`, `docs/milestones/M06-knowledge-and-learning.md` (amendment note), `AGENTS.md` (knowledge line: agent compaction + FTS, no embeddings)
- Modify: `docs/superpowers/specs/2026-09-28-knowledge-agent-compaction-fts-design.md` status → Accepted

- [ ] **Step 1: Update smoke flow**

Configure compaction agent (mock provider, read-only capable test connector) → move ticket to Done → Knowledge page shows waiting count → **Compact now** → running strip → Pending candidate appears → approve → start a Full run on another ticket → "Knowledge used" lists the item.

- [ ] **Step 2: Run smoke on the default stack**

Run: `make compose-up && make bootstrap && make e2e-smoke-m06-knowledge`  
Expected: PASS. Do not use the local compose file.

- [ ] **Step 3: Update docs**

- [ ] **Step 4: Final verification**

Run: `make test && cargo clippy --workspace -- -D warnings && make web-test`  
Expected: all PASS. Then `make clean`.

- [ ] **Step 5: Commit**

`git commit -m "docs(knowledge): agent compaction and FTS retrieval"`
