# Development Guide

## Prerequisites

- Rust (stable) + `cargo`
- Node.js 22 + Yarn (`corepack enable` or `brew install yarn`)
- Docker + Compose (`docker compose` plugin or `docker-compose` standalone)

## Environment

Two compose stacks exist so agents and humans can run Coppice side by side:

| Stack | Compose file | Use |
|-------|--------------|-----|
| Default | `deploy/docker-compose.yml` | Agents, CI, `make e2e-smoke` |
| Local | `deploy/docker-compose.local.yml` | Your day-to-day testing |

Copy the env file that matches your stack:

```bash
# Human local testing (alternate ports)
cp deploy/.env.local.example .env.local

# Default / agent stack
cp deploy/.env.example .env
```

Key variables:

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | Postgres connection string (port differs per stack) |
| `COPPICE_SERVER_URL` | API base URL for CLI `bootstrap` / `health` (local: `http://localhost:8081`) |
| `VITE_API_URL` | Vite API proxy target (`make web-dev-local` sets this to `:8081`) |
| `SESSION_SECRET` | Session cookie signing |
| `COPPICE_BOOTSTRAP_PASSWORD` | First-admin bootstrap password |

Config file defaults: `deploy/config/default.yaml`. Override via env (`COPPICE_*`) or `COPPICE_CONFIG`.

## Local stack (human testing)

Use the **local** compose file — alternate host ports avoid conflicting with the default stack:

```bash
make compose-local-up
make migrate-local
make bootstrap-local
make web-dev-local   # Vite on host with hot reload (separate terminal)
```

`migrate` uses `DATABASE_URL`; `bootstrap` hits the API via `COPPICE_SERVER_URL` (not the DB). Web is not in the local compose file — run Vite on the host for hot reload.

- API: http://localhost:8081/health
- Web: http://localhost:5173 — login `admin@localhost` / `changeme`

Tear down Docker services:

```bash
make compose-local-down
```

Docker host ports: Postgres **5433**, API **8081**. Separate Docker project (`coppice-local`) and volumes from the default stack.

## Default stack (agents / smoke tests)

```bash
make compose-up
make migrate
make bootstrap
```

- API: http://localhost:8080/health
- Web: http://localhost:5173

Tear down: `make compose-down`

Always use Docker Compose via the Makefile — not standalone `docker run`.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` | Local stack (`docker-compose.local.yml`) |
| `make compose-local-down` | Stop local stack |
| `make migrate-local` | Migrate against local Postgres (`:5433`) |
| `make bootstrap-local` | Create admin via API on local port `8081` |
| `make web-dev-local` | Vite dev server → local API on `8081` (hot reload) |
| `make compose-up` | Default stack (`docker-compose.yml`) |
| `make compose-down` | Stop default stack |
| `make migrate` | Migrate against default Postgres (`:5432`) |
| `make bootstrap` | Create admin via API on default port `8080` |
| `make test` | `cargo test --workspace` |
| `make clippy` | `cargo clippy --workspace -- -D warnings` |
| `make web-install` | `yarn install --frozen-lockfile` in `web/` |
| `make web-test` | Install + Vitest unit tests in `web/` |
| `make web-dev` | Install + Vite dev server → default API on `8080` |
| `make web-build` | Install + production SPA build |
| `make e2e-smoke` | Compose up + M02 board smoke script |
| `make release-tar` | Self-contained release tarball |

## Running server or web outside compose

For fast iteration on one package:

```bash
# Server (needs Postgres running via compose-local-up or compose-up)
export DATABASE_URL=postgres://coppice:coppice@localhost:5433/coppice   # local stack
# export DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice  # default stack
cargo run -p coppice-server

# Web dev server (proxies API — see web/vite.config.ts)
make web-dev-local   # with compose-local-up
# make web-dev       # with compose-up (default stack)
```

Migrations and bootstrap always go through the CLI crate (`coppice-cli`).

## CLI commands

```bash
coppice migrate
coppice health
coppice bootstrap admin --email <email> --password <password>
```

## Release build

```bash
make release-tar
```

See `deploy/README-RELEASE.md` for running the tarball.
