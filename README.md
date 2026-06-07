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

## Quick start

```bash
cp deploy/.env.example .env
make compose-up
make migrate
cargo run -p coppice-cli -- bootstrap admin --email admin@localhost --password changeme
curl http://localhost:8080/health
```

Open the web UI at [http://localhost:5173](http://localhost:5173). Sign in with the bootstrap credentials (`admin@localhost` / `changeme`). If bootstrap returns 403, an admin user already exists — use those credentials to log in.

## Release build

Build a self-contained tarball with the API server, CLI, and compiled SPA:

```bash
make release-tar
```

Extract the archive, set `COPPICE_STORAGE__STATIC_DIR=./web/dist`, and run `./coppice-server` on port `:8080`. See `deploy/README-RELEASE.md` for details.
