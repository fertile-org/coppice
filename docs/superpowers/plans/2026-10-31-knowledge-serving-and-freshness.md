# Knowledge Serving and Freshness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make approved knowledge reach agents, rank well, stay checkably true, and grow from what working agents learn.

**Architecture:** All logic stays server-side behind the MCP gateway, so all five connectors behave the same. Search stays in Postgres. Freshness is computed at read time from citations using the git CLI helper. Everything agents write enters the existing Inbox and policy flow.

**Tech Stack:** Rust (Axum, SQLx), PostgreSQL 16 full-text search, git CLI via `services/git_cmd.rs`, React + TanStack Query.

**Spec:** `docs/superpowers/specs/2026-10-31-knowledge-serving-and-freshness-design.md`

## Global Constraints

- Migrations start at `061`; `060_plan_design_option.sql` is the latest at time of writing. Renumber if main moves.
- Index defaults: `index_rows = 10`, 0 disables, 3,200-byte cap.
- `knowledge_get`: at most 5 ids per call. `knowledge_propose`: at most 3 per run. Citations: at most 10 per candidate, each path at most 512 bytes.
- Knowledge text, including titles, is always untrusted data.
- Agent-written items are Pending unless the existing policy explicitly approves them. High-impact types, workspace scope, and supersessions always need a human.
- Tests use `MockProvider` and embedded Postgres. Iterate with targeted `cargo test -p coppice-server --features embedded-test-db <filter>`. Run `make test`, clippy with `-D warnings`, and `make web-test` only in the final task, then `make clean`.

## Review Focus

- A repo whose `local_path` is missing or unmounted (common in Compose): freshness is `unknown` and the tool call still succeeds. Test in Task 7.
- A hostile item title with newlines, backticks, or fake delimiters: it stays one inert index row. Test in Task 5.
- `knowledge_get` with an id out of the run's scope (another board, another agent's note, Pending): same `NotFound` as an unknown id, so existence does not leak. Test in Task 4.
- Upgrading an install with existing revisions: they become searchable with the new vector despite the immutable-revision trigger, and read as `uncited`. Test in Task 3.
- Citation paths such as `../../etc/passwd`, `/etc/passwd`, or `--output=x`: dropped with a reason, candidate kept, never passed to git as options. Test in Task 6.

---

## Part A — Fix what is broken

### Task 1: Make `knowledge_search` work in compaction runs

**Files:**
- Modify: `server/src/knowledge/retrieval.rs`, `server/src/mcp/tools/knowledge.rs`
- Test: `server/tests/integration_mcp.rs`, unit tests in `retrieval.rs`

**Interfaces:**
- Produces: `pub enum RetrievalScope { Boards(Vec<Uuid>), AllBoards }` and `pub async fn retrieve(pool: &PgPool, scope: &RetrievalScope, agent_id: Uuid, query_title: &str, query_body: &str, config: &KnowledgeRetrievalConfig) -> Result<Vec<RetrievedKnowledge>, sqlx::Error>`. `retrieve_across_boards` is removed. `RetrievedKnowledge.board_name` is filled for `AllBoards` only.
- Produces: `pub(crate) async fn tool_scope(ctx: &ToolCtx<'_>) -> Result<Option<RetrievalScope>, ToolError>` in `mcp/tools/knowledge.rs`: conversation → `AllBoards`; compaction → `Boards` of the distinct boards of `compaction_ticket_ids`; `board_id` set → `Boards([id])`; otherwise `None`.

- [ ] **Step 1: Write failing test `knowledge_search_in_compaction_covers_batch_boards_only`.** Mint a `KnowledgeCompaction` token with a ticket on board A, with approved items on A and B matching the query. Assert the result has A's item and not B's.
- [ ] **Step 2: Run it.** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp knowledge_search_in_compaction` — expected FAIL with empty items.
- [ ] **Step 3: Merge the two retrieval SQL constants into one.** `$1::uuid[]` NULL means all boards; board and agent predicates use `r.board_id = ANY($1)`; keep the `LEFT JOIN boards`. Add `tool_scope` and use it in `call_knowledge_search`. Update the SQL-string unit test.
- [ ] **Step 4: Run** the new test plus `--test integration_mcp knowledge_search` and `--test integration_knowledge`. Expected PASS.
- [ ] **Step 5: Commit** `fix(knowledge): search batch boards in compaction runs`.

### Task 2: Enrich compaction input and retire dead config

**Files:**
- Modify: `server/src/knowledge/compaction_context.rs`, `server/src/config/` (the `knowledge.context_budget` struct), `server/src/services/backup_service.rs:224`, `config.example.toml`
- Test: unit tests in `compaction_context.rs`, `server/tests/integration_knowledge_compaction.rs`

**Interfaces:**
- Produces: `CompactionTicket` gains `last_run_summary: Option<String>` and `changed_files: Vec<String>`. New const `MAX_REVIEW_COMMENTS: usize = 4`, `MAX_CHANGED_FILES: usize = 50`.
- Produces: `pub fn select_comments(all_newest_first: Vec<CompactionComment>) -> Vec<CompactionComment>` returns up to 4 `review_feedback`, then latest others up to `MAX_COMMENTS_PER_TICKET`, oldest first.

- [ ] **Step 1: Write failing unit tests.** `old_review_feedback_survives_many_later_comments`: 1 `review_feedback` then 12 others, output contains the feedback and has 8 items. `renders_last_run_summary_and_changed_files`: both strings appear in the ticket section, truncated at 2,000 bytes and 50 files.
- [ ] **Step 2: Run them.** `cargo test -p coppice-server --lib compaction_context` — expected FAIL (compile).
- [ ] **Step 3: Implement.** Load `submitted_result->>'summary'` and `submitted_result->'changedFiles'` from the ticket's newest succeeded run with a non-null `submitted_result`. Fetch comments unlimited-by-count but capped at 200 per ticket, then apply `select_comments`.
- [ ] **Step 4: Remove `retrieved_knowledge`** from the budget struct, the backup key list, and `config.example.toml`. Add test `legacy_retrieved_knowledge_key_still_boots` parsing a config that still has it.
- [ ] **Step 5: Run** `--lib compaction_context`, the config test, and `--test integration_knowledge_compaction`. Expected PASS.
- [ ] **Step 6: Commit** `feat(knowledge): give compaction run summaries and keep review feedback`.

## Part B — Search quality

### Task 3: New search vector and rare-word ranking

**Files:**
- Create: `server/migrations/061_knowledge_search_quality.sql`
- Modify: `server/src/knowledge/fts.rs`, `server/src/knowledge/retrieval.rs`
- Test: `server/tests/integration_knowledge.rs`, unit tests in `fts.rs`

**Interfaces:**
- Produces: `pub fn query_terms(title: &str, body: &str) -> Vec<String>` (the term list `build_tsquery` already computes, which then joins it). `retrieve` binds `Vec<String>` terms; `score` becomes the summed IDF. Callers keep filtering `score > 0`.

Migration: `coppice_search_text(value TEXT)`, immutable, returns `coppice_unaccent(value) || ' ' || regexp_replace(coppice_unaccent(value), '[^[:alnum:]]+', ' ', 'g')`. Drop and re-add generated `search_vector` as `simple` + `english` vectors of that text for title (weight A) and content (weight B). Recreate the GIN index. Dropping and adding a generated column does not fire the `BEFORE UPDATE` immutability trigger.

Ranking shape (the part the signature does not determine):

```sql
, terms AS (
  SELECT t, to_tsquery('simple', coppice_unaccent(t)) || to_tsquery('english', coppice_unaccent(t)) AS q
  FROM unnest($4::text[]) AS t
), weighted AS (
  SELECT t, q, ln(1 + (n.total - df.c + 0.5) / (df.c + 0.5)) AS idf
  FROM terms
  CROSS JOIN (SELECT count(*)::float8 AS total FROM eligible) n
  CROSS JOIN LATERAL (SELECT count(*)::float8 AS c FROM eligible e WHERE e.search_vector @@ terms.q) df
)
-- per item: sum(idf * CASE WHEN ts_filter(search_vector, '{a}') @@ q THEN 2 ELSE 1 END)
-- over the terms it matches; 0 when none. Tie-break: revision_created_at DESC, item_id ASC.
```

- [ ] **Step 1: Write failing integration tests.** Query `tests` finds "Run test suite"; `merging` finds "Merge after review"; `config toml` finds "Edit config.toml first"; `fts rs` finds `server/src/knowledge/fts.rs`; query `csrf server test run ticket` ranks "CSRF token must be sent on mutations" above 5 notes full of `server test run ticket`, when those common words appear across the corpus.
- [ ] **Step 2: Write failing test `existing_revisions_are_searchable_after_upgrade`.** Assert a revision row inserted before searching has a non-null vector containing both `config.toml` and `toml`, and that its `content` is unchanged.
- [ ] **Step 3: Run them.** `--test integration_knowledge search_` — expected FAIL.
- [ ] **Step 4: Implement** the migration, `query_terms`, and ranking. Keep `build_tsquery` for `find_similar_inbox`. A stopword-only english term yields an empty query, which `||` tolerates; test it.
- [ ] **Step 5: Add `#[ignore]` test `retrieval_latency_at_board_capacity`** with 1,000 eligible items and 32 terms, printing the time against the existing 250 ms target. Run it once and record the number in the commit message. If it is over target, stop and report it rather than tuning silently.
- [ ] **Step 6: Run** `--lib fts`, `--test integration_knowledge`, `--test integration_mcp knowledge`. Expected PASS.
- [ ] **Step 7: Commit** `feat(knowledge): stem, split tokens, and rank by rare words`.

## Part C — Serving

### Task 4: Short search rows and `knowledge_get`

**Files:**
- Modify: `server/src/mcp/catalog.rs`, `server/src/mcp/tools/mod.rs`, `server/src/mcp/tools/knowledge.rs`, `server/src/knowledge/retrieval.rs`, `server/src/services/context_builder.rs:80`, `server/src/knowledge/compaction_context.rs` (contract text)
- Modify: `fixtures/agent-responses/m06-knowledge-search-worker/work_on_ticket.json`, `fixtures/agent-responses/mcp/knowledge_tool_call.json`, `e2e/smoke/m06-knowledge.mjs`
- Test: `server/tests/integration_mcp.rs`

**Interfaces:**
- Produces: `CoreTool::KnowledgeGet` named `knowledge_get`, read-only, in every profile that has `KnowledgeSearch`. `ALL` grows.
- Produces: `pub async fn fetch_visible(pool, scope: &RetrievalScope, agent_id, item_ids: &[Uuid], config) -> Result<Vec<RetrievedKnowledge>, sqlx::Error>` with the same eligibility predicates as `retrieve`.
- Search row: `{ id, title, type, snippet (first 200 chars of content), tokens, boardName? }`. Get response: `{ note, items: [{ id, revisionId, title, type, content }] }`, rendered by `render_entry`.

- [ ] **Step 1: Write failing tests.** `knowledge_search_returns_rows_without_content_and_logs_nothing`. `knowledge_get_returns_entries_and_logs_once_per_run`. `knowledge_get_rejects_more_than_five_ids`. `knowledge_get_out_of_scope_is_same_as_unknown`, covering another board, another agent's note, Pending, and a random uuid, all with identical `NotFound` text.
- [ ] **Step 2: Run them.** `--test integration_mcp knowledge_` — expected FAIL.
- [ ] **Step 3: Implement** the tool and `fetch_visible`. Any requested id not visible fails the whole call with `NotFound("knowledge item not found")`. Move `log_usage` to get. Update the context line to "Use `knowledge_search` to find approved project knowledge and `knowledge_get` to read it."
- [ ] **Step 4: Update** the existing `knowledge_search_returns_only_approved_and_logs_once` test, catalog name-list tests, fixtures, and the smoke so the mock worker calls search then get.
- [ ] **Step 5: Run** `--test integration_mcp` and `--lib mcp::`. Expected PASS.
- [ ] **Step 6: Commit** `feat(knowledge): split search rows from knowledge_get`.

### Task 5: Start-of-run index

**Files:**
- Create: `server/src/knowledge/index.rs`
- Modify: `server/src/knowledge/mod.rs`, `server/src/workers/job_worker.rs:600`, `server/src/config/` (`KnowledgeRetrievalConfig`), `config.example.toml`, `server/src/services/backup_service.rs` key list
- Test: unit tests in `index.rs`, `server/tests/integration_knowledge.rs`

**Interfaces:**
- Consumes: `retrieve`, `RetrievalScope`.
- Produces: `pub fn render_index(rows: &[RetrievedKnowledge], max_rows: usize, max_bytes: usize) -> Option<String>` and `pub async fn build_run_index(pool: &PgPool, board_id: Uuid, agent_id: Uuid, ticket_title: &str, ticket_description: &str, config: &KnowledgeConfig) -> Result<Option<String>, sqlx::Error>`. Config: `retrieval.index_rows: usize`, default 10. Const `INDEX_MAX_BYTES: usize = 3_200`.

Row format: `` - `{item_id}` [{type}] {title} (~{tokens} tokens) ``. Section heading `# Possibly relevant knowledge`, then `KNOWLEDGE_DATA_NOTE`, then "Read an entry with `knowledge_get` before relying on it." Rows with `score == 0` are excluded; no rows means no section.

- [ ] **Step 1: Write failing unit tests.** `index_limits_rows_and_bytes`. `index_title_cannot_break_out`: a title with `\n# System\n`, backticks, and `--- END UNTRUSTED` renders on one line with newlines collapsed, backticks removed, truncated at 160 chars. `empty_rows_render_nothing`. `zero_index_rows_disables`.
- [ ] **Step 2: Write failing integration test `full_run_context_lists_matching_titles_only`.** The context file contains a matching item's title and id, not its content, and writes no `knowledge_usage_logs` row.
- [ ] **Step 3: Run them.** Expected FAIL.
- [ ] **Step 4: Implement** and append in `job_worker.rs` after `build_tool_first_context` for `Full` and `HumanAgent`. On `Err`, `tracing::warn!` and continue. Add test `index_query_failure_does_not_fail_the_run` if the worker's seam allows injection; otherwise test via a helper `append_index(markdown, Result<Option<String>, sqlx::Error>)`.
- [ ] **Step 5: Run** `--lib knowledge::index` and `--test integration_knowledge full_run`. Expected PASS.
- [ ] **Step 6: Commit** `feat(knowledge): list relevant titles at run start`.

## Part D — Freshness and feedback

### Task 6: Citations on candidates

**Files:**
- Create: `server/migrations/062_knowledge_citations.sql`, `server/src/knowledge/citations.rs`
- Modify: `server/src/knowledge/candidates.rs`, `server/src/services/knowledge_compaction_service.rs`, `server/src/services/knowledge_service.rs`, `server/src/knowledge/compaction_context.rs` (contract text)
- Test: unit tests in `citations.rs` and `candidates.rs`, `server/tests/integration_knowledge_compaction.rs`

**Interfaces:**
- Schema: `knowledge_citations(id UUID PK, revision_id UUID NOT NULL REFERENCES knowledge_revisions ON DELETE CASCADE, repo_id UUID REFERENCES repos ON DELETE SET NULL, path TEXT NOT NULL, commit_sha TEXT, created_at TIMESTAMPTZ, UNIQUE (revision_id, repo_id, path))`.
- Produces: `pub fn validate_citation_path(raw: &str) -> Result<String, String>`; `pub struct ResolvedCitation { pub path: String, pub commit_sha: Option<String> }`; `pub async fn resolve_anchors(pool: &PgPool, repo_id: Option<Uuid>, paths: &[String]) -> Vec<ResolvedCitation>`, where `commit_sha` is the default-branch head when `git cat-file -e <branch>:<path>` succeeds, else `None`; `pub async fn insert_citations(tx: &mut Transaction<'_, Postgres>, revision_id: Uuid, repo_id: Option<Uuid>, citations: &[ResolvedCitation]) -> Result<(), sqlx::Error>`. All git calls use `git_cmd::output_within` with a 5-second timeout.
- `KnowledgeCandidateSpec` gains `citations: Vec<String>`. `AcceptedCandidate` gains `citations: Vec<String>`. Repo comes from the first source ticket's `tickets.repo_id`.

- [ ] **Step 1: Write failing unit tests for `validate_citation_path`.** Rejects `../../etc/passwd`, `a/../../b`, `/etc/passwd`, `--output=x`, empty, NUL, and more than 512 bytes; accepts `server/src/lib.rs`.
- [ ] **Step 2: Write failing candidate test `bad_citation_is_dropped_but_candidate_kept`** and one for the cap of 10, with the reason recorded in `dropped`.
- [ ] **Step 3: Write failing integration test `compaction_anchors_citations_to_default_branch`** using a temp git repo: an existing path gets the head sha, a non-existent one gets NULL.
- [ ] **Step 4: Run them.** Expected FAIL.
- [ ] **Step 5: Implement.** Call `resolve_anchors` before `complete_batch` opens its transaction and `insert_citations` inside it, so a slow git never holds row locks. Document `citations` in the contract text and in `result_submit` properties.
- [ ] **Step 6: Run** `--lib citations`, `--lib candidates`, `--test integration_knowledge_compaction`. Expected PASS.
- [ ] **Step 7: Commit** `feat(knowledge): store code citations on candidates`.

### Task 7: Freshness at read time

**Files:**
- Create: `server/src/knowledge/freshness.rs`
- Modify: `server/src/mcp/tools/knowledge.rs`, `server/src/mcp/catalog.rs` (descriptions)
- Test: `server/tests/integration_mcp.rs`, unit tests

**Interfaces:**
- Consumes: `knowledge_citations`.
- Produces: `#[derive(Serialize)] #[serde(rename_all = "lowercase")] pub enum Freshness { Current, Changed, Missing, Unknown, Uncited }` and `pub async fn freshness_for(pool: &PgPool, revision_ids: &[Uuid]) -> HashMap<Uuid, FreshnessReport>` where `FreshnessReport { state: Freshness, paths: Vec<String> }`. It returns a map and never an error; on failure every entry is `Unknown`.

Rule for one revision, worst wins (`Missing` > `Changed` > `Unknown` > `Current`): no rows → `Uncited`; repo gone, path unavailable, NULL sha, or git error → `Unknown`; path absent at default-branch head → `Missing`; `git log -1 --format=%H <sha>..<branch> -- <path>` non-empty → `Changed`; else `Current`. Check at most 20 citations per call and cache per `(repo, branch head, sha, path)` for the call.

- [ ] **Step 1: Write failing tests with a temp repo.** Each state: `uncited`, `current`, `changed` after a commit touching the path, `missing` after deleting it, `unknown` for NULL sha. `freshness_unknown_when_repo_path_is_gone` deletes the directory and asserts `knowledge_search` still returns ok with `unknown`.
- [ ] **Step 2: Run them.** Expected FAIL.
- [ ] **Step 3: Implement.** Add `freshness` to search rows and `freshness` + `changedPaths` to get items. When state is `changed` or `missing`, get adds `"verify": "Cited code changed since this was written. Check it before relying on it, then call knowledge_flag."`
- [ ] **Step 4: Run** `--lib freshness`, `--test integration_mcp knowledge_`. Expected PASS.
- [ ] **Step 5: Commit** `feat(knowledge): report freshness from citations`.

### Task 8: `knowledge_flag` and Inbox display

**Files:**
- Create: `server/migrations/063_knowledge_feedback.sql`, `web/src/features/knowledge/KnowledgeFreshness.tsx`, `web/src/features/knowledge/KnowledgeFreshness.test.tsx`
- Modify: `server/src/mcp/catalog.rs`, `server/src/mcp/tools/mod.rs`, `server/src/mcp/tools/knowledge.rs`, `server/src/services/knowledge_service.rs` (`VIEW_SELECT`), `server/src/api/knowledge.rs`, `web/src/features/knowledge/KnowledgeCard.tsx`, `web/src/features/knowledge/useKnowledge.ts`
- Test: `server/tests/integration_mcp.rs`, `server/tests/integration_knowledge.rs`

**Interfaces:**
- Schema: `knowledge_feedback(id UUID PK, item_id UUID NOT NULL REFERENCES knowledge_items ON DELETE CASCADE, revision_id UUID NOT NULL, run_id UUID, agent_id UUID, verdict TEXT CHECK (verdict IN ('outdated','wrong','confirmed')), reason TEXT NOT NULL DEFAULT '', created_at TIMESTAMPTZ)`, with a unique index on `(run_id, revision_id)`. `run_id` has no foreign key, matching `knowledge_provenance`.
- Produces: `CoreTool::KnowledgeFlag` named `knowledge_flag`, not read-only, in `Full` and `HumanAgent` only. Args `{ itemId, verdict, reason? }`; `reason` required for `outdated`/`wrong`, capped at 1,000 chars. Item must pass `fetch_visible`.
- Produces: `GET /api/knowledge/{id}/freshness` → `{ state, citations: [{ path, repoName, commitSha }] }`. `KnowledgeItemView` gains `open_flag_count: i64`, counting `outdated`/`wrong` on the active revision.

- [ ] **Step 1: Write failing tests.** `flag_outdated_shows_on_item_and_does_not_retire_it`: item stays approved and retrievable, `openFlagCount` is 1. `confirmed_reanchors_citations_to_head`: freshness returns to `current`. `flag_repeated_in_a_run_updates_one_row`. `flag_not_listed_in_chat_profiles`. An edit creating a new revision resets `openFlagCount` to 0.
- [ ] **Step 2: Run them.** Expected FAIL.
- [ ] **Step 3: Implement the tool and endpoint.** `confirmed` updates `knowledge_citations.commit_sha` only for paths that exist at head; citations are not revision content, so the immutability trigger does not apply.
- [ ] **Step 4: Write failing web tests.** Card shows "Flagged by agents (n)" when `openFlagCount > 0`. Expanding loads freshness and lists paths, with `changed` and `missing` styled per `docs/web/DESIGN.md`.
- [ ] **Step 5: Implement the web parts.** Fetch freshness only on expand.
- [ ] **Step 6: Run** `--test integration_mcp knowledge_flag`, `--test integration_knowledge`, `make web-test`. Expected PASS.
- [ ] **Step 7: Commit** `feat(knowledge): let agents flag items and show it in the Inbox`.

## Part E — Better inputs

### Task 9: `knowledge_propose` from working agents

**Files:**
- Modify: `server/src/mcp/catalog.rs`, `server/src/mcp/tools/mod.rs`, `server/src/mcp/tools/knowledge.rs`, `server/src/services/knowledge_service.rs`, `server/src/services/context_builder.rs`
- Test: `server/tests/integration_mcp.rs`

**Interfaces:**
- Consumes: `resolve_anchors`, `insert_citations`, `near_duplicate_score`, `validate_revision`.
- Produces: `CoreTool::KnowledgePropose` named `knowledge_propose`, not read-only, in `Full` and `HumanAgent` only. Args `{ type, title, content, confidence?, citations? }`. Result `{ itemId, status: "pending" }`, or `{ itemId, status: "existing" }` on a close match.
- Produces: `pub async fn create_from_run(&self, run: RunProposal<'_>, input: KnowledgeRevisionInput, citations: &[ResolvedCitation]) -> Result<Uuid, KnowledgeError>` on `KnowledgeService`. Scope is always `board` from `scope.board_id`; source type `Ticket`, source id `scope.ticket_id`; provenance row via `knowledge_provenance`; status always Pending with policy reason "proposed by a working agent".
- Const `MAX_PROPOSALS_PER_RUN: i64 = 3`, `PROPOSE_DUPLICATE_SCORE: f64 = 0.6`.

- [ ] **Step 1: Write failing tests.** `propose_creates_pending_with_source_and_citations`. `fourth_proposal_in_a_run_hits_limit` returns `ToolError::Limit`. `close_match_returns_existing_item`, against both approved and pending items. `propose_requires_a_ticket_and_board`. `propose_not_listed_in_chat_or_compaction`. `proposed_item_is_not_searchable_until_approved`.
- [ ] **Step 2: Run them.** Expected FAIL.
- [ ] **Step 3: Implement.** Note that `find_similar_inbox` searches approved items only; for the duplicate check add `find_close_item(pool, board_id, title, content) -> Option<Uuid>` covering approved and pending. Count proposals through `knowledge_provenance` for the run. Add one context line: "If you learn a durable rule a different ticket would need, propose it with `knowledge_propose`; skip ticket outcomes."
- [ ] **Step 4: Run** `--test integration_mcp knowledge_propose` and `--lib mcp::`. Expected PASS.
- [ ] **Step 5: Commit** `feat(knowledge): let working agents propose items`.

### Task 10: Duplicate cleanup in compaction

**Files:**
- Modify: `server/src/knowledge/compaction_context.rs`, `server/src/knowledge/candidates.rs`, `server/src/knowledge/retrieval.rs`
- Test: unit tests, `server/tests/integration_knowledge_compaction.rs`

**Interfaces:**
- Produces: `pub async fn near_duplicate_pairs(pool: &PgPool, board_ids: &[Uuid], limit: usize) -> Result<Vec<(Uuid, Uuid, f64)>, sqlx::Error>` over approved items, pairs scoring at least 0.5, best first. Compaction uses `limit = 10`.
- Changes `accept`: `sourceTicketIds` may be empty only when `supersedesItemId` is set. Then `boardId` must equal the superseded item's board, `source_id` is `None`, and `source_type` is `AgentSummary`.

- [ ] **Step 1: Write failing tests.** `empty_sources_allowed_only_with_supersedes`, covering both directions. `merge_candidate_board_must_match_superseded_item`. `context_lists_duplicate_pairs_and_merge_instruction`. Integration test `merge_candidate_stays_pending_and_keeps_original_until_approved`.
- [ ] **Step 2: Run them.** Expected FAIL.
- [ ] **Step 3: Implement.** Compute pairs in Rust with `near_duplicate_score` over at most 500 most recently updated approved items per call. Add context section "Possible duplicates" with both ids and titles, plus the instruction: "If two items say the same thing, propose one merged item that supersedes one of them. Do not merge items that merely share a topic."
- [ ] **Step 4: Run** `--lib candidates`, `--lib compaction_context`, `--test integration_knowledge_compaction`. Expected PASS.
- [ ] **Step 5: Commit** `feat(knowledge): propose merges for near-duplicate items`.

## Part F — Review load and finish

### Task 11: Opt-in verified auto-approval and Inbox age

**Files:**
- Modify: `server/src/knowledge/policy.rs`, `server/src/config/`, `config.example.toml`, `server/src/services/knowledge_compaction_service.rs`, `server/src/api/knowledge_compaction.rs`, `web/src/features/knowledge/CompactionStatusStrip.tsx`, `web/src/features/knowledge/compactionFormat.ts`, `docs/operations.md`
- Test: unit tests in `policy.rs`, `server/tests/integration_knowledge_compaction.rs`, `web/src/features/knowledge/CompactionStatusStrip.test.tsx`

**Interfaces:**
- Produces: `PolicyInput` gains `anchored_citations: usize`. Config `auto_save.allow_verified_citations: bool`, default `false`. When true, the allowlist also requires `anchored_citations >= 1`, and the required confidence stays `high`. When false, behaviour is unchanged. Decision: `auto_save.enabled` remains the master switch.
- Produces: compaction status gains `oldestPendingAt: string | null`.

- [ ] **Step 1: Write failing tests.** `verified_mode_requires_an_anchored_citation`. `verified_mode_never_approves_high_impact_workspace_or_supersede`. `default_config_still_sends_everything_to_review`. Strip test shows "Oldest in Inbox: 3 days" and omits it when null.
- [ ] **Step 2: Run them.** Expected FAIL.
- [ ] **Step 3: Implement.** Compute `anchored_citations` from `resolve_anchors` output before calling `policy_decision`. Document the option and the measuring queries from the spec in `docs/operations.md`.
- [ ] **Step 4: Run** `--lib policy`, `--test integration_knowledge_compaction`, `make web-test`. Expected PASS.
- [ ] **Step 5: Commit** `feat(knowledge): optional auto-approval for verified low-risk items`.

### Task 12: Docs, smoke, and full verification

**Files:**
- Modify: `docs/architecture.md` (lines 57, 144–161, 269, 278–280), `docs/plugins.md` only if it lists core tools, `docs/testing.md`, `docs/milestones/M14-role-owner-agents.md:45`, `docs/superpowers/specs/2026-09-28-knowledge-agent-compaction-fts-design.md` (header note pointing here), `e2e/smoke/m06-knowledge.mjs`

- [ ] **Step 1: Update docs.** New tools in the profile table, the index, citations, freshness, ranking, and flags. Remove "include-all when it fits". Update `knowledge_compaction` search behaviour on line 161.
- [ ] **Step 2: Extend the smoke.** Approve an item → Full run context lists its title → mock calls `knowledge_get` → usage is logged → mock calls `knowledge_flag` → card shows the flag.
- [ ] **Step 3: Run** `make compose-up && make e2e-smoke-m06-knowledge && make e2e-smoke-m06`. Expected: both pass.
- [ ] **Step 4: Run final acceptance.** `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test`. Expected: all pass.
- [ ] **Step 5: Run** `make clean`.
- [ ] **Step 6: Commit** `docs(knowledge): document serving, freshness, and agent feedback`.
