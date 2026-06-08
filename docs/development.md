# Development Guide

## Prerequisites

- Rust (stable) + `cargo`
- Node.js 22 + npm
- Docker (for Postgres and full-stack local run)

## Environment

Copy the example env and adjust if needed:

```bash
cp deploy/.env.example .env
```

Key variables (also used by CI):

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | Postgres connection string |
| `SESSION_SECRET` | Session cookie signing |
| `COPPICE_BOOTSTRAP_PASSWORD` | First-admin bootstrap password |

Config file defaults: `deploy/config/default.yaml`. Override via env (`COPPICE_*`) or `COPPICE_CONFIG`.

## Local stack (preferred)

Always use Docker Compose via the Makefile — not standalone `docker run`:

```bash
make compose-up      # postgres + server + web
make migrate         # apply SQL migrations
cargo run -p coppice-cli -- bootstrap admin --email admin@localhost --password changeme
```

- API: http://localhost:8080/health
- Web: http://localhost:5173 — login `admin@localhost` / `changeme`

Tear down:

```bash
make compose-down
```

Compose file: `deploy/docker-compose.yml`. Services: `postgres` (pgvector/pg16), `server`, `web`.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-up` | `docker compose up -d --build` |
| `make compose-down` | Stop compose stack |
| `make migrate` | `coppice migrate` |
| `make test` | `cargo test --workspace` |
| `make clippy` | `cargo clippy --workspace -- -D warnings` |
| `make web-test` | Vitest unit tests in `web/` |
| `make web-dev` | Vite dev server (without compose web container) |
| `make web-build` | Production SPA build |
| `make e2e-smoke` | Compose up + M02 board smoke script |
| `make release-tar` | Self-contained release tarball |

## Running server or web outside compose

For fast iteration on one package:

```bash
# Server (needs Postgres running via compose)
export DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice
cargo run -p coppice-server

# Web dev server (proxies API — see web/vite.config.ts)
make web-dev
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
