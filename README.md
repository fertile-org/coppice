# Coppice

**Coppice — grow an agent team from shared roots.**

Coppice is a lightweight, self-hosted workspace where AI agents work through tickets, communicate through comments, learn from project history, operate inside sandboxes, and proactively raise engineering signals.

## Monorepo layout

| Folder | Purpose |
|--------|---------|
| `server/` | Rust API server |
| `web/` | React SPA |
| `cli/` | Rust CLI (`coppice`) |
| `config/` | Shared TOML config loader |
| `deploy/` | Docker Compose and deployment config |
| `docs/` | Architecture, milestones, and design docs |
| `e2e/` | End-to-end tests |
| `fixtures/` | Test fixtures and sample data |

Contributing or using an AI agent? See [AGENTS.md](AGENTS.md).

## Quick start (local development)

Prerequisites: Rust, Node.js 22 + Yarn, Docker Compose. For API hot reload: `cargo install cargo-watch`.

```bash
cp config.example.toml config.toml
```

Config layers built-in defaults, then `~/.config/coppice/config.toml`, then `./config.toml` (local overrides global). Use `localhost:5433` for Postgres — the port Docker publishes for the local stack.

### Step 1 — Database

```bash
make compose-local-up
make migrate
```

### Step 2 — API (hot reload)

In a separate terminal:

```bash
make server-dev-local
```

First time only (while the API is running):

```bash
make bootstrap
```

API health: http://localhost:8080/health

### Step 3 — Web (hot reload)

In another terminal:

```bash
make web-dev
```

Open [http://localhost:5173](http://localhost:5173) and sign in with `admin@localhost` / `changeme`.

| Service | Port | Where |
|---------|------|--------|
| Postgres | 5433 | Docker (`make compose-local-up`) |
| API | 8080 | Host (`make server-dev-local`) |
| Web | 5173 | Host (`make web-dev`) |

Stop Postgres: `make compose-local-down`

## Production / release

After `make release-tar`, copy `config.example.toml` to `config.toml`, then:

```bash
coppice migrate
coppice bootstrap admin --email admin@localhost --password changeme
coppice server start   # API on :8080
coppice web start      # UI on :5173 (proxies /api to the API)
```

Example systemd units are in `deploy/systemd/`. See `deploy/README-RELEASE.md`.

More commands and config details: [docs/development.md](docs/development.md).

## Default stack (agents / CI)

The default compose file uses standard ports for agents, smoke tests, and CI:

```bash
make compose-up    # server auto-migrates on start
make bootstrap     # first time only
```

- API: http://localhost:8080/health
- Web: http://localhost:5173

The Docker server uses `deploy/config/default.toml` inside the container — not your repo `config.toml`.

## Release build

```bash
make release-tar
```

Extract the archive, configure `config.toml`, then run `coppice server start` and `coppice web start`. See `deploy/README-RELEASE.md` for details.
