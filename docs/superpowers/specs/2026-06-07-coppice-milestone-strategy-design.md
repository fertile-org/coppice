# Coppice Milestone Strategy Design

**Date:** 2026-06-07  
**Status:** Approved  
**Product:** Coppice — grow an agent team from shared roots.

> Coppice is a lightweight, self-hosted workspace where AI agents work through tickets, communicate through comments, learn from project history, operate inside sandboxes, and proactively raise engineering signals.

## Purpose

The product design in `docs/philosophy/final_agent_workspace_product_design.md` describes the full v1 product. Implementing it in one pass is impractical. This document defines how that design is split into **seven implementation milestones**, each with its own spec, docker-compose delta, and test strategy.

Milestones are **not release gates**. You start daily use only when all milestones are complete. Each milestone must leave the repo in a working, tested state so implementation can proceed incrementally.

## Decisions

| Topic | Decision |
|-------|----------|
| Split strategy | Restructure product design phases into ~7 milestones with clear dependencies |
| Delivery shape | M1 infra/API only; M2 onward full-stack (API + UI + tests) |
| Milestone docs | `docs/milestones/M0N-*.md` |
| Agent testing | `MockProvider` implements `AgentProvider`; real CLI adapters swap in via config later |
| Auth | Session login (httpOnly cookie, argon2, CSRF) from M1 API / M2 UI — safety-first |
| E2E | CI runs agent-browser **smoke** subset; full suite via local `make e2e` |
| Environment | `docker compose up` is the only supported dev/CI path |

## Milestone overview

| Milestone | Title | One-line goal |
|-----------|-------|---------------|
| M01 | Foundation | Postgres, auth API, mock provider trait, CI harness |
| M02 | Workspace & board | Login UI, board, tickets, comments, agent config |
| M03 | Agent execution | Job queue, worktrees, mock runs, result contract |
| M04 | Live console | tmux/PTY, WebSocket terminal, log artifacts |
| M05 | Workflow & collaboration | Rules, mentions, clarification, final human gate |
| M06 | Knowledge & learning | pgvector, retrieval, extractor, knowledge inbox |
| M07 | Trust & signals | Capabilities, sandbox, secrets, workspace inbox, git/PR |

Detailed scope, acceptance criteria, and test matrices: see `docs/milestones/`.

## Repo layout

```text
coppice/
  server/                 # Rust / Axum / SQLx
  web/                    # React / Vite SPA
  deploy/
    docker-compose.yml
  docs/
    philosophy/
    milestones/
    superpowers/specs/
  e2e/
    smoke/                # CI agent-browser scripts
    full/                 # local make e2e scripts
  scripts/
    bootstrap-admin.sh
```

## Technical stack

From `docs/philosophy/final_agent_workspace_framework_selection.md`:

- **Backend:** Rust, Axum, Tokio, SQLx, PostgreSQL 16+, pgvector
- **Frontend:** React, Vite, TanStack Query, dnd-kit, xterm.js, Tailwind, Radix/shadcn
- **Deploy:** Docker Compose (postgres + server + web)
- **Job queue:** Postgres-backed `agent_jobs` table (no Redis in v1)

## Agent provider adapter

All agent execution goes through a single trait. Tests and default dev compose use `MockProvider`, which returns deterministic dummy text/JSON matching the result contract. Real CLI adapters (Claude Code, Codex, OpenCode, OpenClaw, shell) are added later without changing orchestration code — only config and a new adapter impl.

```yaml
agents:
  default_provider: mock
  providers:
    mock:
      adapter: mock
      responses_dir: ./fixtures/agent-responses
```

## Auth model

- M1: session auth API (login, logout, `/me`), argon2, httpOnly secure cookie, CSRF on mutations, bootstrap first admin via env/script
- M2: login-gated SPA; only `/health` and auth routes are public
- All integration and E2E tests authenticate via session cookie

## Docker Compose evolution

| Milestone | Services / volumes |
|-----------|---------------------|
| M01 | `postgres` (pgvector), `server` |
| M02 | + `web`; artifact/worktree volume paths reserved |
| M03+ | `server` mounts `/data/artifacts`, `/data/worktrees`, `/data/repos` |

Every milestone must work with `docker compose up` alone — no manual Postgres install.

## Testing conventions

| Layer | Tooling | When |
|-------|---------|------|
| Rust unit | `cargo test` | CI every PR |
| Rust integration | compose Postgres | CI every PR |
| FE unit | Vitest | CI every PR |
| E2E smoke | agent-browser, `e2e/smoke/` | CI every PR |
| E2E full | agent-browser, `e2e/full/`, `make e2e` | Local pre-merge |

Rules:

- Agent runs in automated tests always use `MockProvider`.
- Integration tests never mock the database — use compose Postgres.
- E2E smoke requires backend running locally via compose; FE hits real API.

## Mapping to product design build phases

| Product design §24 phase | Milestone |
|--------------------------|-----------|
| Phase 1 Core board & agents | M01 + M02 |
| Phase 2 Knowledge v1 | M06 |
| Phase 3 Agent runner | M03 |
| Phase 4 Live console | M04 |
| Phase 5 Comments, mentions, workflow | M02 (comments) + M05 |
| Phase 6 Capabilities, sandbox, secrets | M07 |
| Phase 7 Proactive signals | M07 |
| Phase 8 Learning extractor | M06 |
| Phase 9 PR integrations | M07 (minimal) |

## Milestone doc template

Each file under `docs/milestones/` includes: Goal, Product scope, Out of scope, Dependencies, Architecture notes, Docker Compose delta, Testing strategy, Acceptance criteria, References.

## Non-goals (all milestones)

- Multi-tenant SaaS, enterprise RBAC, agent marketplace
- Kubernetes, Redis queue, separate vector DB
- Real CLI providers in CI
- Scheduled observation cron (manual Run Observation in M7 only)
- Autonomous production deployment

## Next step

After spec review, invoke the writing-plans skill to produce an implementation plan for **M01 — Foundation**.
