# Coppice — Agent Guide

**Coppice** is a self-hosted agent workspace: Trello-like board, tickets, comments, and (from M03) agent execution. Philosophy and full product design live in `docs/philosophy/`.

**Status:** M05 workflow & collaboration is complete. **Next:** [M06 — Knowledge & learning](docs/milestones/M06-knowledge-and-learning.md).

## Must read before coding

1. **Milestones are sequential.** Read the current milestone spec in `docs/milestones/` before implementing. Do not skip acceptance criteria or pull scope from later milestones.
2. **Docker Compose is the dev path — use the default stack only.** Agents and CI must use `deploy/docker-compose.yml` via `make compose-up`, `make bootstrap`, `make e2e-smoke`, and `make e2e-smoke-m03`. That stack runs Postgres, server, and web together (ports 5432, 8080, 5173). The server container auto-migrates on start and does not read the repo `config.toml`. **Do not use the human local stack** (`deploy/docker-compose.local.yml`, `make compose-local-up`, `make server-dev`, `make web-dev`) — that alternate path exists for human hot-reload dev (Postgres on 5433, API/web on host). If smoke or integration tests fail, fix the default stack or free ports 5432/8080/5173; do not fall back to the local compose file. Do not start Postgres or services with ad-hoc `docker run`. See [docs/development.md](docs/development.md).
3. **Server owns state.** API handlers are thin; business rules live in `server/src/services/` and `server/src/domain/`. The SPA does not invent status transitions or workflow rules.
4. **Auth is session + CSRF.** httpOnly cookie sessions; mutations require `X-CSRF-Token`. Integration tests authenticate via session cookie (see `server/tests/common/mod.rs`).
5. **Agent tests use `MockProvider`.** No real CLI adapters in CI or automated tests until configured manually. Fixtures: `fixtures/agent-responses/`.
6. **CI must pass.** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `make web-test`. Clippy warnings are errors.
7. **Repositories.** Admin registers git checkouts by `local_path` (Settings → Repositories). Coppice creates worktrees only — no server-side `git clone`. Optional `remote_url` for metadata/PR (M07). Bind-mount host repos into the server container in Docker.
8. **Agent execution env.** `AGENT_DEFAULT_PROVIDER` (default `mock`), `WORKTREES_PATH`, `AGENT_WORKER_COUNT`. Smoke: `make e2e-smoke-m03`.

## Monorepo (quick map)

| Path | Role |
|------|------|
| `server/` | Rust API (Axum, SQLx) |
| `web/` | React SPA (Vite, TanStack Query) |
| `cli/` | Operator CLI (`coppice migrate`, `bootstrap`, `health`) |
| `deploy/` | Docker Compose + Dockerfiles |
| `e2e/` | Browser smoke tests |
| `docs/` | Philosophy, milestones, dev docs |

## Read on demand

| Topic | Doc |
|-------|-----|
| Local setup & commands | [docs/development.md](docs/development.md) |
| Code layout & conventions | [docs/architecture.md](docs/architecture.md) |
| Testing strategy | [docs/testing.md](docs/testing.md) |
| Roadmap & acceptance criteria | [docs/milestones/README.md](docs/milestones/README.md) |
| Product principles & UX | [docs/philosophy/final_agent_workspace_product_design.md](docs/philosophy/final_agent_workspace_product_design.md) |
| Stack choices | [docs/philosophy/final_agent_workspace_framework_selection.md](docs/philosophy/final_agent_workspace_framework_selection.md) |
| Web visual design | [docs/web/DESIGN.md](docs/web/DESIGN.md) |
