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
  middleware/   Session auth, CSRF, admin checks
  providers/    AgentProvider trait + MockProvider
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

- PostgreSQL 16 + pgvector image.
- Migrations: `server/migrations/*.sql`, applied by `coppice migrate` and on test connect (`db::connect_and_migrate`).
- No Redis; agent job queue uses Postgres `agent_jobs` (M03).
- **M03 tables:** `agent_runs` (one row per ticket+agent execution; statuses `queued`/`running`/`completed`/`failed`/`cancelled`; unique partial index on active `(ticket_id, agent_id)`), `agent_jobs` (queue row per run; `FOR UPDATE SKIP LOCKED` claim by workers).

## Auth

- Session cookie (httpOnly), argon2 password hashes.
- Public routes: `/health`, `/api/auth/login`, `/api/auth/bootstrap`.
- Protected routes: session middleware + CSRF on mutations (`X-CSRF-Token` from login response).
- Roles: `admin` / `member` — admin-only routes use `middleware/admin.rs`.

## Agent execution (M03)

All agent execution goes through `AgentProvider`; orchestration lives in services + workers:

```text
providers/mod.rs          trait + AgentRunResult contract
providers/mock.rs         deterministic fixtures from fixtures/agent-responses/
services/run_service.rs   create/cancel/finish runs
services/job_service.rs   enqueue, claim (SKIP LOCKED), mark done/failed
services/worktree_service.rs   git clone + worktree per (ticket, agent)
services/context_builder.rs    write .agent/context.md into worktree
services/result_contract.rs    apply nextStatus, comments, blocker metadata
workers/job_worker.rs     poll queue, run pipeline, spawn at server startup
```

**Run pipeline (worker):** claim pending job → load run/ticket/agent/repo → mark running → ensure repo clone (`GIT_REPOS_PATH/{repo-id}/`) → ensure worktree (`WORKTREES_PATH/TICKET-{id}-{agent}-{repo}/`) → write context file → call `AgentProvider::run` → apply result contract → finish run.

**Config env:** `AGENT_DEFAULT_PROVIDER`, `GIT_REPOS_PATH`, `WORKTREES_PATH`, `AGENT_WORKER_COUNT` (see `deploy/docker-compose.yml`). Swap real CLI adapters via provider config — do not fork orchestration per provider.

## Web frontend

```text
web/src/
  features/     auth, board, tickets, agents, projects, users
  components/   AppShell, ProtectedRoute, shared UI
  lib/          api.ts (fetch + CSRF), schemas/ (Zod), query-client
  styles/       tokens.css (design tokens)
```

- **Routing:** React Router; `/login` public, everything else behind `ProtectedRoute`.
- **Data:** TanStack Query hooks per feature (`useTickets`, `useAgents`, …).
- **API client:** `lib/api.ts` — `credentials: 'include'`, CSRF header on writes.
- **Board:** fixed columns in `features/board/columns.ts`; dnd-kit for drag-and-drop.
- **Forms:** React Hook Form + Zod schemas in `lib/schemas/`.

Visual design tokens and palette: `docs/web/DESIGN.md`.

## CLI

`cli/` — thin clap wrapper around migrate, health, bootstrap. Shares DB/config conventions with server but no HTTP.

## Config & artifacts

- Default config: `deploy/config/default.yaml`
- Attachments: filesystem under `COPPICE_STORAGE__ARTIFACTS_DIR` (compose volume `artifact_data`)
- Static SPA (release): `COPPICE_STORAGE__STATIC_DIR` served by Axum fallback

## Milestone evolution

Each milestone adds modules/tables/endpoints documented in `docs/milestones/M0N-*.md`. After M03 the server has projects, repos, tickets, comments, attachments, agents, users, agent runs/jobs, worktrees, and in-process workers. **Next:** M04 live console — see that spec before implementing.
