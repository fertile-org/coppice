# Coppice — Agent Guide

**Coppice** is a local desktop agent workspace: Trello-like board, tickets, comments, and (from M03) agent execution. It ships as an Electron app that bundles PostgreSQL, the Rust server and the SPA, so end users install nothing else (`.dmg` / `.deb` built from a git tag; Docker Compose remains the dev/CI stack). Philosophy and full product design live in `docs/philosophy/`.

**Status:** M01–M09 are complete (M07 was narrowed to git/PR + forge secrets). [M10 — Plugins](docs/milestones/M10-plugins.md) Parts 1, 2a, and 2b are implemented: tool-first harness (MCP gateway `/mcp`, core tools, built-in skills), plugin dirs / git install / enablement / plugin skills, and the plugin MCP proxy (stdio + HTTP, encrypted settings, Test button, Tools & Skills tab, console tool titles). M10 still open: live verification of the `claude-code`, `codex`, and `kilo-code` gateway wiring (manual acceptance, needs their CLIs). [M11 — Desktop release](docs/milestones/M11-desktop-release.md) is implemented (`coppice-server desktop` with bundled Postgres 16, Electron shell, electron-builder packaging, tag-triggered release workflow); still open: the live tag check and manual install acceptance with a real connector (MockProvider is not in desktop or release builds). **Next implement:** [M12 — Beta Release](docs/milestones/M12-beta-release.md) (public site, Electron-first docs, macOS arm64 `.dmg` and Linux x64 `.deb`), still blocked by those M11 leftovers and by M10's manual connector verification. Then [M13 — Security & sandbox](docs/milestones/M13-security-and-sandbox.md), then [M14 — Role-owner agents](docs/milestones/M14-role-owner-agents.md).

## Must read before coding

1. **Milestones are sequential.** Read the current milestone spec in `docs/milestones/` before implementing. Do not skip acceptance criteria or pull scope from later milestones.
2. **Docker Compose is the dev path — use the default stack only.** Agents and CI must use `deploy/docker-compose.yml` via `make compose-up`, `make bootstrap`, `make e2e-smoke`, and `make e2e-smoke-m03`. That stack runs Postgres, server, and web together (ports 5432, 5000, 5001). The **web** image is production (`vite build` + nginx, proxies `/api`/`/ws` to the API). The server container auto-migrates on start and does not read the repo `config.toml`. **Do not use the human local stack** (`deploy/docker-compose.local.yml`, `make compose-local-up`, `make server-dev`, `make web-dev`) — that alternate path exists for human hot-reload dev (Postgres on 5433, API/web on host). If smoke or integration tests fail, fix the default stack or free ports 5432/5000/5001; do not fall back to the local compose file. Do not start Postgres or services with ad-hoc `docker run`. See [docs/development.md](docs/development.md). The desktop app is an additional path, not a replacement: verify packaging with `make desktop-test` and `make desktop-dist-dir desktop-smoke` ([desktop/README.md](desktop/README.md)).
3. **Server owns state.** API handlers are thin; business rules live in `server/src/services/` and `server/src/domain/`. The SPA does not invent status transitions or workflow rules.
4. **Auth is session + CSRF.** httpOnly cookie sessions; mutations require `X-CSRF-Token`. Integration tests authenticate via session cookie (see `server/tests/common/mod.rs`).
5. **Agent tests use `MockProvider`.** Mock is for CI and automated tests only, not an end-user connector. Fixtures: `fixtures/agent-responses/`. Desktop and release builds omit the `mock-provider` feature ([docs/providers/mock.md](docs/providers/mock.md)). The five real connectors (Claude Code, Codex, Cursor, OpenCode, Kilo Code) run as host CLIs. Docker is only needed for the Compose dev stack.
6. **CI must pass.** `make test` (embedded Postgres, no Docker), `cargo clippy --workspace -- -D warnings`, and `make web-test` (plus `make desktop-test` when touching `desktop/`). Clippy warnings are errors. After a **successful** full Rust test pass for your task, run `make clean` to reclaim disk (`target/` can grow to 10+ GB). Do **not** run it before every incremental `cargo test` during development — only when you are done with the task.
7. **Fast verification during work.** Do **not** run `make test` while iterating — even in parallel (cargo-nextest; serial fallback without it) the full suite builds every integration binary. Use `make test-unit` (lib only), `make test-smoke` (lib + a few integration files), targeted `cargo test -p coppice-server --features embedded-test-db <filter>`, or `make web-test`. Reserve `make test` for final acceptance. Rust tests do **not** require `make compose-up` or `DATABASE_URL`.
8. **Repositories.** Admin registers git checkouts by `local_path` (Settings → Repositories). Coppice creates worktrees only — no server-side `git clone`. Optional `remote_url` for metadata/PR (M07, done). Bind-mount host repos into the server container in Docker.
9. **Agent execution env.** `WORKTREES_PATH`, `AGENT_WORKER_COUNT`. (`AGENT_DEFAULT_PROVIDER` is obsolete and ignored.) Smoke: `make e2e-smoke-m03`. Context long-running (`continued`, `splitTickets`): [design spec](docs/superpowers/specs/2026-06-10-context-long-running-tasks-design.md); smoke: `make e2e-smoke-m06`. Governed knowledge: [design spec](docs/superpowers/specs/2026-08-03-m06-knowledge-and-learning-design.md), amended by [agent compaction + full-text search](docs/superpowers/specs/2026-09-28-knowledge-agent-compaction-fts-design.md) (no embeddings; an admin-selected agent compacts Done tickets in batches); smoke: `make e2e-smoke-m06-knowledge`. The five real connectors run as host CLIs. In Compose, sign in with `coppice connector …` on the server container ([M08](docs/milestones/M08-connector-operator-cli.md)) instead of bind-mounting host CLIs. Runs are **tool-first**: `.agent/context.md` is slim and agents read tickets, knowledge and skills through the Coppice MCP gateway at `POST /mcp`, authenticated by a per-run token (`COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN`) and finished with `result_submit`. Connectors configure it per run only — never the worktree or a global CLI config — and fail with `mcp_unavailable` when they cannot ([per-connector status](docs/providers/README.md#coppice-mcp-gateway-tool-first-runs)). Plugins: Claude Code / Cursor format folders; their skills load as `<plugin>:<skill>` and their MCP servers are proxied through the gateway as `<plugin>__<tool>` ([architecture](docs/architecture.md#plugin-mcp-proxy)); smoke: `make e2e-smoke-m10`.

## Monorepo (quick map)

| Path | Role |
|------|------|
| `server/` | Rust API (Axum, SQLx) |
| `web/` | React SPA (Vite, TanStack Query) |
| `cli/` | Operator CLI (`coppice migrate`, `bootstrap`, `health`, `connector`) |
| `connectors/` | Static connector descriptors (ids, capabilities, run contract, MCP wiring, console kind) shared by server and CLI |
| `desktop/` | Electron shell, electron-builder packaging, release scripts (spawns `coppice-server desktop`) |
| `deploy/` | Docker Compose + Dockerfiles |
| `e2e/` | Browser smoke tests |
| `docs/` | Contributor docs: philosophy, milestones, development |
| `website/` | Astro marketing site and user docs (M12). Primary product docs; `docs/` stays for contributors |

## Read on demand

| Topic | Doc |
|-------|-----|
| Local setup & commands | [docs/development.md](docs/development.md), [docs/operations.md](docs/operations.md) |
| Code layout & conventions | [docs/architecture.md](docs/architecture.md) |
| Testing strategy | [docs/testing.md](docs/testing.md) |
| Using & writing plugins | [docs/plugins.md](docs/plugins.md), example: `examples/plugins/hello-coppice` |
| Roadmap & acceptance criteria | [docs/milestones/README.md](docs/milestones/README.md) |
| Product principles & UX | [docs/philosophy/final_agent_workspace_product_design.md](docs/philosophy/final_agent_workspace_product_design.md) |
| Stack choices | [docs/philosophy/final_agent_workspace_framework_selection.md](docs/philosophy/final_agent_workspace_framework_selection.md) |
| Web visual design | [docs/web/DESIGN.md](docs/web/DESIGN.md) |
