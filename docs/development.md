# Development Guide

## Prerequisites

- Rust (stable) + `cargo`
- `cargo-watch` for local API hot reload (`cargo install cargo-watch`)
- Node.js 22 + Yarn (`corepack enable` or `brew install yarn`)
- Docker + Compose (`docker compose` plugin or `docker-compose` standalone)

## Configuration

Coppice uses TOML config files — not `.env` files.

| Location | Priority | Purpose |
|----------|----------|---------|
| Built-in defaults | lowest | Sensible dev defaults in `coppice-config` |
| `~/.config/coppice/config.toml` | middle | Per-user global settings |
| `./config.toml` (cwd) | higher | Local / per-install overrides (gitignored) |
| `COPPICE_CONFIG` file | higher | Explicit file path (Docker / release) |
| Environment variables | highest | Container overrides (`DATABASE_URL`, `COPPICE_*`, …) |

Copy the example for local development:

```bash
cp config.example.toml config.toml
```

Key fields for local dev (`config.toml`):

| Field | Purpose |
|-------|---------|
| `database.url` | Host → Docker Postgres on `localhost:5433` |
| `server.port` | API listen port (`8080`) |
| `auth.session_secret` | Session cookie signing |
| `auth.bootstrap_password` | First-admin bootstrap password |
| `storage.artifacts_dir` | Upload storage on host |
| `agent.worktrees_path` | Agent worktrees on host |

Docker Compose (agents / CI) sets `COPPICE_CONFIG=/etc/coppice/config.toml` (from `deploy/config/default.toml` in the image) plus env overrides in `deploy/docker-compose.yml`. The server container does **not** read your repo `config.toml`.

### Agent stack vs human `config.toml`

| | Agent / CI (`make compose-up`) | Human hot reload (`compose-local-up` + host API) |
|--|--|--|
| Postgres port | 5432 | 5433 |
| API | Docker `:8080` | Host `:8080` |
| Config source | `default.toml` + compose env inside container | `./config.toml` on the host |
| Migrations | Server auto-migrates on container start | `make migrate` (reads `config.toml`) |

Your gitignored `config.toml` (e.g. `database.url` → `:5433`) applies only to **host** CLI and `coppice-server` when run on the host. It does not affect the Docker server. Avoid running host `make migrate` against the agent stack unless you override the URL, e.g. `DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice make migrate` — otherwise you may migrate the wrong database.

## Local development (human)

Postgres runs in Docker on port **5433**. API and web run on the host for hot reload.

```bash
cp config.example.toml config.toml

# Step 1 — Database
make compose-local-up
make migrate

# Step 2 — API (separate terminal)
make server-dev-local
make bootstrap   # first time only

# Step 3 — Web (separate terminal)
make web-dev
```

- API: http://localhost:8080/health
- Web: http://localhost:5173 — login `admin@localhost` / `changeme`

Tear down Postgres: `make compose-local-down`

## Release / installed binary

```bash
cp config.example.toml config.toml   # or ~/.config/coppice/config.toml
coppice migrate
coppice bootstrap admin --email admin@localhost --password changeme
coppice server start   # API
coppice web start      # SPA + /api proxy (recommended for self-hosting)
```

| Command | Role |
|---------|------|
| `coppice server start` | Runs `coppice-server` (API + workers) |
| `coppice web start` | Serves `web/dist` and proxies `/api` to the API |

Set `COPPICE_SERVER_BIN` to override the API binary path.

**systemd:** example units in `deploy/systemd/`.

## Default stack (agents / smoke tests)

```bash
make compose-up    # server auto-migrates on start
make bootstrap     # first time only
```

Tear down: `make compose-down`

Always use Docker Compose via the Makefile — not standalone `docker run`.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` | Start local Postgres only (port 5433) |
| `make compose-local-down` | Stop local Postgres |
| `make server-dev-local` | API with `cargo watch` (hot reload) |
| `make compose-up` | Default Docker stack (agents / CI) |
| `make compose-down` | Stop default stack |
| `make migrate` | `coppice migrate` (reads `config.toml` on host) |
| `make bootstrap` | `coppice bootstrap admin` |
| `make web-dev` | Vite dev server (proxies to `:8080`) |
| `make test` | `cargo test --workspace` |
| `make clippy` | `cargo clippy --workspace -- -D warnings` |
| `make release-tar` | Self-contained release tarball |

## CLI commands

All CLI commands load the same config as the server:

```bash
coppice migrate
coppice health
coppice health --check-database
coppice bootstrap admin --email <email> --password <password>
coppice server start
coppice web start
```

## Release build

```bash
make release-tar
```

See `deploy/README-RELEASE.md` for running the tarball.
