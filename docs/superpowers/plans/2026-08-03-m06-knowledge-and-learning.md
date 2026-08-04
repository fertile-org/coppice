# M06 Knowledge and Learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver bounded, human-governed, revisioned agent knowledge with durable extraction/embedding, scoped retrieval, context budgeting, usage audit, and a complete web inbox.

**Architecture:** Stable lifecycle rows point at immutable current and active revisions. Dedicated PostgreSQL jobs perform extraction and embedding; Full runs synchronously embed their query, retrieve from a metadata-filtered pgvector set, render untrusted entries under a strict token budget, and atomically log exact usage snapshots. Thin Axum handlers expose the service, while React Query drives the Knowledge screen and per-run audit disclosure.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL 16 + pgvector, Tokio, Reqwest, React 19, TypeScript, TanStack Query, Zod, Vitest, Docker Compose.

**Design:** `docs/superpowers/specs/2026-08-03-m06-knowledge-and-learning-design.md`

---

### Task 1: Configuration and persistence contract

**Files:**
- Create: `server/migrations/013_knowledge_learning.sql`
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`
- Modify: `deploy/config/default.toml`
- Modify: `deploy/docker-compose.yml`
- Modify: `server/src/db/pool.rs`

- [ ] **Step 1: Add failing configuration tests**

Add tests that load defaults, reject an auto-save type outside the fixed low-risk allowlist, and expose a 1536 embedding dimension with hard list/retrieval limits.

- [ ] **Step 2: Run the configuration tests and verify failure**

Run: `cargo test -p coppice-config knowledge -- --nocapture`  
Expected: FAIL because `KnowledgeConfig` does not exist.

- [ ] **Step 3: Add typed TOML configuration**

Define `KnowledgeConfig`, `EmbeddingConfig`, `ExtractionConfig`, `AutoSaveConfig`, `RetrievalConfig`, and `ContextBudgetConfig`; add defaults and a `validate()` method. Call validation after Figment extraction.

- [ ] **Step 4: Add the migration**

Create `knowledge_items`, `knowledge_revisions`, `knowledge_embeddings vector(1536)`, `knowledge_usage_logs`, and `knowledge_jobs`, including checks, foreign keys, immutable-revision trigger, Done-transition extraction trigger, keyset/retrieval indexes, and job dedupe indexes.

- [ ] **Step 5: Include knowledge tables in test reset**

Add all knowledge tables to `truncate_test_workspace` before their referenced parent tables.

- [ ] **Step 6: Run config and migration-focused tests**

Run: `cargo test -p coppice-config knowledge && cargo test -p coppice-server --features embedded-test-db db:: -- --nocapture`  
Expected: PASS.

- [ ] **Step 7: Commit**

Run: `git add config/src/lib.rs config.example.toml deploy/config/default.toml deploy/docker-compose.yml server/migrations/013_knowledge_learning.sql server/src/db/pool.rs && git commit -m "feat(knowledge): add M06 persistence contract"`

### Task 2: Knowledge domain, policy, and lifecycle service

**Files:**
- Create: `server/src/domain/knowledge.rs`
- Create: `server/src/services/knowledge_service.rs`
- Modify: `server/src/domain/mod.rs`
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Write failing domain tests**

Cover enum round-trips, title/content bounds, scope ownership, high-impact approval, explicit low-risk allowlist, cursor parsing, and `expectedVersion` conflicts.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p coppice-server knowledge --lib -- --nocapture`  
Expected: FAIL because the knowledge modules are absent.

- [ ] **Step 3: Implement domain types and validation**

Add `KnowledgeScope`, `KnowledgeType`, `KnowledgeStatus`, `KnowledgeConfidence`, `KnowledgeRevisionInput`, and conversion/validation helpers. Enforce `workspace`, `project`, and `agent` scope invariants and reject `team`.

- [ ] **Step 4: Implement lifecycle transactions**

Add service methods `create_manual`, `list`, `approve`, `edit`, `reject`, `mark_stale`, `expire`, and `supersede`. Each mutation locks the item, compares `expected_version`, appends revisions rather than updating them, increments version, and schedules embedding without clearing the prior active revision.

- [ ] **Step 5: Implement stable keyset pagination and response loading**

Order by `updated_at DESC, id DESC`, fetch `limit + 1`, clamp 1–100, and return an opaque cursor. Join current/active revision, source, embedding/job state, usage count, and supersession metadata.

- [ ] **Step 6: Run domain/service tests**

Run: `cargo test -p coppice-server knowledge --lib -- --nocapture`  
Expected: PASS.

- [ ] **Step 7: Commit**

Run: `git add server/src/domain server/src/services && git commit -m "feat(knowledge): add governed lifecycle service"`

### Task 3: Embedding/extraction providers and durable worker

**Files:**
- Create: `server/src/knowledge/mod.rs`
- Create: `server/src/knowledge/embedder.rs`
- Create: `server/src/knowledge/mock_embedder.rs`
- Create: `server/src/knowledge/openai_embedder.rs`
- Create: `server/src/knowledge/extractor.rs`
- Create: `server/src/services/knowledge_job_service.rs`
- Create: `server/src/workers/knowledge_worker.rs`
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `server/src/workers/mod.rs`

- [ ] **Step 1: Write failing provider and job tests**

Cover deterministic normalized vectors, output ordering, non-finite/dimension/count rejection, bounded extractor input, risky Pending policy, low-risk explicit auto-save, SKIP LOCKED claiming, retry, activation, and failed replacement preservation.

- [ ] **Step 2: Run provider tests and verify failure**

Run: `cargo test -p coppice-server knowledge --lib -- --nocapture`  
Expected: FAIL because providers and worker do not exist.

- [ ] **Step 3: Implement providers**

Define `EmbeddingProvider` and build mock/OpenAI-compatible implementations. Send `input`, `model`, `dimensions`, and `encoding_format: "float"`; validate every vector exactly. Define `ExtractionProvider` and the bounded deterministic extractor.

- [ ] **Step 4: Implement durable job operations**

Claim with `FOR UPDATE SKIP LOCKED`, reclaim stale running jobs, and mark success/retry/failure with bounded backoff. Embed jobs upsert the embedding then activate only if the revision remains current and approved. Extraction jobs load bounded ticket/comment/review data and create candidates idempotently.

- [ ] **Step 5: Validate migrated dimension at startup**

Read `format_type` for `knowledge_embeddings.embedding` and fail with configured/migrated values if they differ. Build providers only after config and schema validation.

- [ ] **Step 6: Spawn dedicated workers**

Add provider handles to `AppState`, update all test-state constructors, and start configured knowledge workers separately from agent workers.

- [ ] **Step 7: Run provider/worker tests**

Run: `cargo test -p coppice-server knowledge --lib --features embedded-test-db -- --nocapture`  
Expected: PASS.

- [ ] **Step 8: Commit**

Run: `git add server/src && git commit -m "feat(knowledge): add durable learning workers"`

### Task 4: Lifecycle and audit API

**Files:**
- Create: `server/src/api/knowledge.rs`
- Modify: `server/src/api/mod.rs`
- Modify: `server/src/api/agent_runs.rs`
- Create: `server/tests/integration_knowledge.rs`
- Modify: `server/tests/common/mod.rs`

- [ ] **Step 1: Write failing HTTP integration cases**

Exercise authentication, CSRF, admin mutation authorization, create/list pagination, every lifecycle operation, stale versions, source/provenance preservation, and per-run usage snapshots.

- [ ] **Step 2: Run and verify endpoint failures**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_knowledge -- --nocapture`  
Expected: FAIL with missing routes.

- [ ] **Step 3: Implement thin handlers and DTOs**

Expose the design's routes, parse/validate strings in the service/domain layer, map conflicts to 409, validation to 400, not-found to 404, and return camelCase JSON. Require `AdminUser` on mutations and `AuthUser` on reads.

- [ ] **Step 4: Implement run knowledge audit endpoint**

Return immutable usage rows ordered by rank then revision ID and verify the run exists before returning an empty list.

- [ ] **Step 5: Run integration tests**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_knowledge -- --nocapture`  
Expected: PASS.

- [ ] **Step 6: Commit**

Run: `git add server/src/api server/tests && git commit -m "feat(api): expose knowledge lifecycle and audit"`

### Task 5: Retrieval, context budgeting, and run usage

**Files:**
- Create: `server/src/knowledge/retrieval.rs`
- Create: `server/src/services/context_budget.rs`
- Modify: `server/src/services/context_builder.rs`
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/src/knowledge/mod.rs`
- Extend: `server/tests/integration_knowledge.rs`

- [ ] **Step 1: Write failing retrieval and budget tests**

Cover every eligibility filter, deterministic tie order, top-k clamp, untrusted delimiters, optional-section trimming, mandatory overflow failure, exact final cap, and duplicate usage insertion.

- [ ] **Step 2: Run and verify failures**

Run: `cargo test -p coppice-server context_budget --lib && cargo test -p coppice-server --features embedded-test-db --test integration_knowledge retrieval -- --nocapture`  
Expected: FAIL because retrieval/budget integration is absent.

- [ ] **Step 3: Implement metadata-first retrieval**

Embed the bounded ticket query, use a materialized eligibility CTE, rank exact cosine distance, apply stable ties and threshold/top-k limits, then render only entries fitting the knowledge budget.

- [ ] **Step 4: Enforce the whole context budget**

Implement `TokenCounter`, `ByteTokenCounter`, safe UTF-8 truncation, mandatory-section preflight, optional-section budgets, final recount, and a structured over-budget error.

- [ ] **Step 5: Integrate Full run context and usage audit**

Before writing `.agent/context.md`, retrieve and budget knowledge. After the final document is built but before provider invocation, insert exact rendered snapshots with `ON CONFLICT DO NOTHING`. Human profiles skip retrieval.

- [ ] **Step 6: Add representative query-plan assertion**

Seed a representative eligibility mix and use `EXPLAIN (FORMAT JSON)` to assert the lifecycle/scope indexes are available to the materialized stage and no unbounded vector-first scan appears.

- [ ] **Step 7: Run targeted server tests**

Run: `cargo test -p coppice-server context --lib && cargo test -p coppice-server --features embedded-test-db --test integration_knowledge -- --nocapture`  
Expected: PASS.

- [ ] **Step 8: Commit**

Run: `git add server/src server/tests/integration_knowledge.rs && git commit -m "feat(knowledge): retrieve bounded context with usage audit"`

### Task 6: Knowledge Inbox web screen

**Files:**
- Create: `web/src/lib/schemas/knowledge.ts`
- Create: `web/src/features/knowledge/useKnowledge.ts`
- Create: `web/src/features/knowledge/KnowledgePage.tsx`
- Create: `web/src/features/knowledge/KnowledgePage.test.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/components/AppShell.tsx`

- [ ] **Step 1: Write failing schema and interaction tests**

Cover response parsing, tab/status query selection, project filter, keyset load-more, manual candidate creation, all lifecycle actions with the visible version, source opening, and conflict/error messaging.

- [ ] **Step 2: Run and verify failures**

Run: `cd web && yarn test KnowledgePage`  
Expected: FAIL because the feature is absent.

- [ ] **Step 3: Implement schemas and query hooks**

Add strict Zod schemas and TanStack Query hooks. Invalidate status/project lists after mutation and preserve API 409 messages for the user.

- [ ] **Step 4: Build the field-notebook Inbox**

Add `/knowledge`, main navigation, Pending/Approved/Rejected/Stale tabs, project filter, manual candidate form, metadata-rich cards, stable load-more, source ticket action, and accessible lifecycle controls for approve/edit/reject/supersede/stale/expire.

- [ ] **Step 5: Run frontend tests**

Run: `cd web && yarn test KnowledgePage`  
Expected: PASS.

- [ ] **Step 6: Commit**

Run: `git add web/src && git commit -m "feat(web): add Knowledge Inbox"`

### Task 7: Knowledge Used inside Agent Runs

**Files:**
- Create: `web/src/features/knowledge/useKnowledgeUsed.ts`
- Create: `web/src/features/knowledge/KnowledgeUsed.tsx`
- Modify: `web/src/features/tickets/TicketRunsTab.tsx`
- Modify: `web/src/features/tickets/TicketRunsTab.test.tsx`

- [ ] **Step 1: Write the failing run-disclosure tests**

Cover collapsed loading, empty audit, ranked snapshots, revision/source/similarity/token metadata, and endpoint failure.

- [ ] **Step 2: Run and verify failure**

Run: `cd web && yarn test TicketRunsTab`  
Expected: FAIL because Knowledge Used is absent.

- [ ] **Step 3: Implement the audit disclosure**

Fetch `/api/agent-runs/:id/knowledge-used` only when expanded and render the exact server snapshots inside each run card with clear empty/error states.

- [ ] **Step 4: Run frontend tests and build**

Run: `make web-test && make web-build`  
Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add web/src && git commit -m "feat(web): show knowledge used by agent runs"`

### Task 8: Default-Compose acceptance smoke and final verification

**Files:**
- Create: `e2e/smoke/m06-knowledge.mjs`
- Modify: `Makefile`
- Modify: `docs/milestones/M06-knowledge-and-learning.md`
- Modify: `docs/architecture.md`
- Modify: `docs/development.md`

- [ ] **Step 1: Add the distinct knowledge smoke**

Use authenticated API setup on the default Compose stack to create/approve knowledge, wait for embedding readiness, start a mock Full run, assert a usage snapshot, and confirm the Knowledge web route renders. Add `e2e-smoke-m06-knowledge` without changing `e2e-smoke-m06`.

- [ ] **Step 2: Run focused verification**

Run: `make test-unit`  
Expected: PASS.

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_knowledge -- --nocapture`  
Expected: PASS.

Run: `make web-test && make web-build`  
Expected: PASS.

Run: `cargo clippy --workspace -- -D warnings`  
Expected: PASS with no warnings.

- [ ] **Step 3: Run Compose acceptance and regression smokes**

Run: `make e2e-smoke-m06-knowledge`  
Expected: PASS.

Run: `make e2e-smoke-m06`  
Expected: PASS unchanged.

- [ ] **Step 4: Update milestone and architecture documentation**

Mark verified M06 criteria, document the new modules/config/worker/API, and add the new Make target to development commands.

- [ ] **Step 5: Review and commit**

Run: `git diff --check && git status --short`  
Expected: no whitespace errors and only intended tracked changes plus Coppice-managed `.agent/context.md`.

Run: `git add Makefile e2e docs server web config config.example.toml deploy Cargo.lock && git commit -m "feat: deliver M06 governed knowledge"`
