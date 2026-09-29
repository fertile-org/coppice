# Architecture

## Overview

Coppice is a monorepo with three deliverables and shared deploy/test tooling:

```text
server/   Rust API — Axum, SQLx, Tokio
web/      React SPA — Vite, TanStack Query, Tailwind
cli/      Rust operator CLI (workspace member)
deploy/   Docker Compose, Dockerfiles, default config
```

Rust workspace: root `Cargo.toml` with members `server`, `cli`. `web/` is an independent Node package.

## Server layers

```text
server/src/
  api/          HTTP routes, request/response DTOs (thin handlers)
  services/     Business logic, DB queries, validation orchestration
  domain/       Entity types, enums, pure validation helpers
  db/           Pool setup, migration runner
  knowledge/    Full-text retrieval, compaction context and candidate contract (M06)
  middleware/   Session auth, CSRF, admin checks
  providers/    AgentProvider trait + mock / opencode / claude-code / codex / cursor connectors
  workers/      In-process Tokio job workers (M03)
  storage/      Filesystem artifact store (attachments)
  config/       Figment-based AppConfig
```

**Request flow:** `api/*` → `services/*` → SQLx / `storage/*`. Handlers extract auth via `AuthUser`, get a pool from `AppState`, call a service, map errors to HTTP status.

**Error pattern:** Services return typed errors (`thiserror`, e.g. `TicketError`). API maps them to `StatusCode` + JSON message. Use `anyhow` only at CLI/worker boundaries.

**State:** `AppState` holds `AppConfig`, optional `PgPool`, and `AttachmentStore`. Router built in `lib.rs::app()`.

## Domain conventions

- DB columns and Rust enums use `snake_case` (e.g. `in_progress`, `waiting_for_agent`).
- JSON API responses use `camelCase` (`#[serde(rename_all = "camelCase")]` on DTOs).
- IDs are `Uuid` everywhere.
- Timestamps: `time::OffsetDateTime`, serialized as RFC3339 strings in API.
- Ticket status/substatus validation lives in `domain/substatus.rs` and `domain/ticket.rs` — keep rules there, not in handlers.

## Database

- PostgreSQL 16 (pgvector image, kept because early migrations create the extension; `unaccent` comes from contrib).
- Migrations: `server/migrations/*.sql`, applied by `coppice migrate` and on test connect (`db::connect_and_migrate`).
- No Redis; agent job queue uses Postgres `agent_jobs` (M03).
- **M03 tables:** `agent_runs` (one row per ticket+agent execution; statuses `queued`/`running`/`completed`/`failed`/`cancelled`; unique partial index on active `(ticket_id, agent_id)`), `agent_jobs` (queue row per run; `FOR UPDATE SKIP LOCKED` claim by workers).
- **M06 tables:** `knowledge_items` (mutable lifecycle pointer), immutable `knowledge_revisions` (generated `search_vector` + GIN index), `knowledge_item_sources` (source tickets), `knowledge_usage_logs` (unique run/revision audit snapshot with full-text `score`), `workspace_settings` (compaction agent), and the compaction tables `knowledge_compaction_queue`, `knowledge_compaction_batches` (at most one queued/running), `knowledge_compaction_batch_tickets`. Compaction runs are ordinary `agent_runs` rows with `compaction_batch_id` set.

## Auth

- Session cookie (httpOnly), argon2 password hashes.
- Public routes: `/health`, `/api/auth/login`, `/api/auth/bootstrap`.
- Protected routes: session middleware + CSRF on mutations (`X-CSRF-Token` from login response).
- Roles: `admin` / `member` — admin-only routes use `middleware/admin.rs`.

## Agent execution (M03)

All agent execution goes through `AgentProvider`; orchestration lives in services + workers:

```text
providers/mod.rs          trait + AgentRunResult contract
providers/registry.rs     ConnectorRegistry — builds providers from config
providers/mock.rs         deterministic fixtures from fixtures/agent-responses/
providers/opencode.rs     HTTP serve-mode connector (host testing, API keys)
providers/claude_code.rs  subprocess connector (claude -p, host-managed auth)
providers/codex.rs         subprocess connector (codex exec, host-managed auth)
providers/cursor.rs        subprocess connector (agent -p, host-managed auth)
services/run_service.rs   create/cancel/finish runs
services/job_service.rs   enqueue, claim (SKIP LOCKED), mark done/failed
services/repo_service.rs       global registered repos (local_path, verify)
services/worktree_service.rs   worktree per (ticket, agent) from registered local_path
services/context_builder.rs    write .agent/context.md into worktree
services/result_contract.rs    apply nextStatus, comments, blocker metadata
workers/job_worker.rs     poll queue, run pipeline, spawn at server startup
```

**Registered repositories:** Admin registers operator-managed git checkouts via `local_path` (instance-wide). Optional `remote_url` for display and future PR APIs. Coppice does **not** `git clone`. See [M03 registered repositories spec](superpowers/specs/2026-06-08-m03-registered-repositories-design.md).

**Run pipeline (worker):** claim pending job → load run/ticket/agent/repo → validate repo `local_path` → mark running → ensure worktree from registered path (`WORKTREES_PATH/TICKET-{id}-{agent}-{repo}/`) → write context file → call `AgentProvider::run` → apply result contract → finish run.

## Governed knowledge (M06)

Knowledge keeps lifecycle state separate from content. An edit inserts an immutable revision and advances `current_revision_id`; approving (or editing an approved item) activates that revision immediately. Approve, edit, reject, supersede, stale, and expire operations require an optimistic `expectedVersion`.

```text
api/knowledge.rs                          authenticated reads; admin + CSRF lifecycle writes
api/knowledge_compaction.rs               compaction status, Compact now, Retry, Cancel
api/settings.rs                           GET/PUT compaction agent (PUT is admin-only)
domain/knowledge.rs                       types, scope and content validation, risk classification
domain/knowledge_compaction.rs            batch status/trigger, scheduling and byte-budget helpers
services/knowledge_service.rs             revision/lifecycle invariants, compaction inserts, similar items
services/knowledge_compaction_service.rs  queue, batches, drain cycles, completion, failure reconcile
services/workspace_settings_service.rs    compaction agent setting (read-only connectors only)
knowledge/fts.rs                          OR-joined tsquery builder (unaccent, simple config)
knowledge/retrieval.rs                    eligibility CTE, include-all when it fits, else ts_rank_cd top-k
knowledge/compaction_context.rs           bounded `.agent/context.md` for a compaction run
knowledge/candidates.rs                   lenient parse + strict validation of `knowledgeCandidates`
knowledge/policy.rs                       fail-closed approval policy
services/context_budget.rs                ByteTokenCounter, untrusted delimiters, usage snapshots
workers/knowledge_compaction_scheduler.rs reconcile finished runs, scheduled and drain cycles
workers/job_worker/compaction.rs          executes `compact_knowledge` runs
```

**Retrieval.** Only Full-profile runs retrieve knowledge. The query is the ticket title and description, normalized and OR-joined into a `to_tsquery('simple', …)`. Relational eligibility (approved, active, unexpired, unsuperseded, confidence, board/agent scope) is materialized first. If every eligible item fits the context budget, all are included in a stable order (score `0`); otherwise items are ranked by `ts_rank_cd`. Zero matches is not an error. The result is rendered as untrusted data inside the total context budget, and every included exact revision is logged once in `knowledge_usage_logs` before the provider runs.

**Compaction.** A trigger queues a ticket when it enters Done and removes its unbatched row when it leaves Done. The scheduler starts a drain cycle every `knowledge.compaction.interval_secs` (or on Compact now) when an enabled compaction agent is configured. Each batch is one `compact_knowledge` run of that agent: read-only tools, a scratch directory, no repository, no knowledge retrieval, no ticket side effects. The agent returns `knowledgeCandidates`; invalid candidates (sources outside the batch, board mismatch, unknown types, limits) are dropped with reasons in the batch summary. Valid ones go through the fail-closed policy: workspace scope, supersessions, and high-impact types always need human approval. Success deletes the batch's queue rows and sends no notification. The scheduler's reconcile step fails batches whose run ended without applying a result: tickets return to the queue with `attempts + 1`, and a `knowledge_compaction_failed` notification goes to every user. Cancelled runs release tickets without an attempt or notification. Tickets at `max_attempts` wait for a manual Retry. Operator settings: [Knowledge configuration](operations.md#knowledge-configuration).

**Config env:** `AGENT_DEFAULT_PROVIDER`, `WORKTREES_PATH`, `AGENT_WORKER_COUNT` (see `deploy/docker-compose.yml`). Operator bind-mounts host clones; register in-container paths in Settings → Repositories.

## Web frontend

```text
web/src/
  features/     auth, boards, board, tickets, agents, knowledge, users
  components/   AppShell, ProtectedRoute, shared UI
  lib/          api.ts (fetch + CSRF), schemas/ (Zod), query-client
  styles/       tokens.css (design tokens)
```

- **Routing:** React Router; `/login` public, everything else behind `ProtectedRoute`.
- **Data:** TanStack Query hooks per feature (`useTickets`, `useAgents`, …).
- **API client:** `lib/api.ts` — `credentials: 'include'`, CSRF header on writes.
- **Board:** fixed columns in `features/board/columns.ts`; dnd-kit for drag-and-drop.
- **Knowledge:** `/knowledge` has Pending, Approved, Rejected, and Stale views with provenance and lifecycle controls. Expanded Agent Run details load the immutable **Knowledge Used** audit.
- **Forms:** React Hook Form + Zod schemas in `lib/schemas/`.

Visual design tokens and palette: `docs/web/DESIGN.md`.

## CLI

`cli/` — operator CLI: migrate, health, bootstrap, `server start`, `web start`. Shares TOML config with the server. `coppice web start` serves the built SPA and proxies `/api` to the API.

## Config & artifacts

- Host/release: `config.toml` (see root `config.example.toml`); Docker Compose: `deploy/config/config.toml` (see `deploy/config/config.example.toml`), bind-mounted as `COPPICE_CONFIG`
- Attachments: filesystem under `storage.artifacts_dir` (compose volume `artifact_data`)
- Static SPA (release): `coppice web start` via `[web].static_dir`

## Milestone evolution

Each milestone adds modules/tables/endpoints documented in `docs/milestones/M0N-*.md`. Through M06 the system includes boards, repositories, tickets, collaboration workflow, live agent runs, governed long-term knowledge, and bounded/auditable context assembly. M07–M09 add git/PR actions with forge secrets, managed connectors, and Agent Chat. **Next:** M10 plugins, then M11 security & sandbox, then M12 role-owner agents.
