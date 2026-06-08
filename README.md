# Coppice

**Coppice — grow an agent team from shared roots.**

Coppice is a lightweight, self-hosted workspace where AI agents work through tickets, communicate through comments, learn from project history, operate inside sandboxes, and proactively raise engineering signals.

## Monorepo layout

| Folder | Purpose |
|--------|---------|
| `server/` | Rust API server |
| `web/` | React SPA |
| `cli/` | Rust CLI (`coppice`) |
| `deploy/` | Docker Compose and deployment config |
| `docs/` | Architecture, milestones, and design docs |
| `e2e/` | End-to-end tests |
| `fixtures/` | Test fixtures and sample data |

Contributing or using an AI agent? See [AGENTS.md](AGENTS.md).

## Quick start (local testing)


```bash
cp deploy/.env.local.example .env.local
make compose-local-up
make migrate-local
make bootstrap-local
make web-dev-local    # Vite with hot reload (separate terminal)
curl http://localhost:8081/health
```

`bootstrap` calls the HTTP API (default port 8080). The local stack exposes the API on **8081** — `make bootstrap-local` sets `COPPICE_SERVER_URL` for you.

Open the web UI at [http://localhost:5173](http://localhost:5173). Sign in with `admin@localhost` / `changeme`. If bootstrap returns 403, an admin user already exists — use those credentials to log in.

Stop the stack: `make compose-local-down`

| Service | Local port | How |
|---------|------------|-----|
| Postgres | 5433 | Docker (`compose-local-up`) |
| API | 8081 | Docker (`compose-local-up`) |
| Web | 5173 | Host (`make web-dev-local`) |

## Default stack (agents / CI)

The default compose file uses standard ports for agents, smoke tests, and CI:

```bash
cp deploy/.env.example .env
make compose-up
make migrate
make bootstrap
```

- API: http://localhost:8080/health
- Web: http://localhost:5173

More commands and env details: [docs/development.md](docs/development.md).

## Release build

Build a self-contained tarball with the API server, CLI, and compiled SPA:

```bash
make release-tar
```

Extract the archive, set `COPPICE_STORAGE__STATIC_DIR=./web/dist`, and run `./coppice-server` on port `:8080`. See `deploy/README-RELEASE.md` for details.
