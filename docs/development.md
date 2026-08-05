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
| Built-in defaults | lowest | Sensible defaults in `coppice-config` |
| `~/.config/coppice/config.toml` | middle | Per-user global settings (host installs) |
| `./config.toml` (cwd) | higher | Host / hot-reload overrides (gitignored; from root `config.example.toml`) |
| `deploy/config/config.toml` | Docker | Compose bind-mount (gitignored; from `deploy/config/config.example.toml`) |
| `COPPICE_CONFIG` file | higher | Explicit file path |
| Environment variables | highest | Container overrides (`DATABASE_URL`, `COPPICE_*`, …) |

**Host / hot-reload:**

```bash
cp config.example.toml config.toml
```

**Docker Compose:**

```bash
cp deploy/config/config.example.toml deploy/config/config.toml
```

`make compose-up` creates `deploy/config/config.toml` from the example when missing. After editing that file, recreate the server (no image rebuild): `docker compose -f deploy/docker-compose.yml up -d --force-recreate server`.

Key fields for local host dev (`./config.toml`):

| Field | Purpose |
|-------|---------|
| `database.url` | Host → Docker Postgres on `localhost:5433` |
| `server.port` | API listen port (`5000`) |
| `auth.session_secret` | Session cookie signing |
| `auth.bootstrap_password` | Shared secret for the HTTP `/api/auth/bootstrap` gate |
| `auth.bootstrap_admin_email` | Optional: auto-create this admin on server start if users is empty |
| `auth.bootstrap_admin_password` | Optional: login password for auto-created admin |
| `storage.artifacts_dir` | Upload storage on host |
| `agent.worktrees_path` | Agent worktrees on host |
| `knowledge.embedding` | Provider/model and fixed migrated vector dimension |
| `knowledge.retrieval` | Confidence, stable top-k/page bounds, and scope capacities |
| `knowledge.context_budget` | Total and per-section token budgets for Full runs |

Docker Compose bind-mounts `deploy/config/config.toml` at `COPPICE_CONFIG=/etc/coppice/config.toml`, plus env overrides in `deploy/docker-compose.yml`. The server container does **not** read the repo-root `config.toml`.

### Agent stack vs human `config.toml`

| | Agent / CI (`make compose-up`) | Human hot reload (`compose-local-up` + host API) |
|--|--|--|
| Postgres port | 5432 | 5433 |
| API | Docker `:5000` | Host `:5000` |
| Config source | `deploy/config/config.toml` + compose env | `./config.toml` on the host |
| Migrations | Server auto-migrates on container start | `make migrate` (reads `config.toml`) |

Your gitignored repo-root `config.toml` (e.g. `database.url` → `:5433`) applies only to **host** CLI and `coppice-server` when run on the host. It does not affect the Docker server. Avoid running host `make migrate` against the agent stack unless you override the URL, e.g. `DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice make migrate` — otherwise you may migrate the wrong database.

## Local development (human)

Postgres runs in Docker on port **5433**. API and web run on the host for hot reload.

```bash
cp config.example.toml config.toml

# Step 1 — Database
make compose-local-up
make migrate

# Step 2 — API (separate terminal)
make server-dev
make bootstrap   # first time only (host config has no auto-bootstrap by default)

# Step 3 — Web (separate terminal)
make web-dev
```

- API: http://localhost:5000/health
- Web: http://localhost:5001 — login `admin@localhost` / `changeme`

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
make compose-up    # copies deploy/config/config.toml if missing; auto-migrates + auto-bootstraps admin
```

Docker config (`deploy/config/config.toml`, from `config.example.toml` in that folder) can set `auth.bootstrap_admin_email` / `auth.bootstrap_admin_password`. Host installs without those fields still use `make bootstrap` (or `coppice bootstrap admin`) once.

Tear down: `make compose-down`

Always use Docker Compose via the Makefile — not standalone `docker run`.

### Host repos for agents

The server bind-mounts host git checkouts at `/repos`:

| Env / path | Meaning |
|------------|---------|
| `COPPICE_REPOS_HOST` | Host directory (default `$HOME/coppice/repos`) |
| `/repos/<name>` | Path to register in Settings → Repositories |
| `COPPICE_UID` / `COPPICE_GID` | Host user the API runs as (default `1000`; `make compose-up` uses `id -u` / `id -g`) |

Clone or symlink projects into that host directory, then register `/repos/<name>` (the in-container path). Coppice creates worktrees under `/data/worktrees`; it does not `git clone` from `remote_url`.

The server entrypoint starts as root only long enough to `chown` `/data/*` volumes, then drops to `COPPICE_UID`/`COPPICE_GID` so Git ownership matches the bind mount and new files under `/repos` stay yours on the host. If an older root-owned run left files behind: `sudo chown -R "$(id -u):$(id -g)" ~/coppice/repos`.

### Host port overrides

The default stack binds host ports 5432 / 5000 / 5001. If another project owns one of them (common on busy dev machines), override the host-side mapping without touching the file:

```bash
COPPICE_PG_PORT=55432 \
COPPICE_SERVER_PORT=15000 \
COPPICE_WEB_PORT=15001 \
COPPICE_API_URL=http://localhost:15000 \
COPPICE_WEB_URL=http://localhost:15001 \
  make e2e-smoke
```

Only the host-side mapping changes; container-internal ports and the `postgres` service DNS are unaffected, so `DATABASE_URL` and `VITE_API_URL` inside the stack stay the same. The smoke scripts read `COPPICE_API_URL` / `COPPICE_WEB_URL` (defaults `:5000` / `:5001`), so set those to match when you move the server/web ports.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` | Start local Postgres only (port 5433) |
| `make compose-local-down` | Stop local Postgres |
| `make server-dev` | API with `cargo watch` (hot reload) |
| `make compose-up` | Default Docker stack (agents / CI) |
| `make compose-down` | Stop default stack |
| `make migrate` | `coppice migrate` (reads `config.toml` on host) |
| `make bootstrap` | `coppice bootstrap admin` |
| `make web-dev` | Vite dev server (proxies to `:5000`) |
| `make test` | Full Rust suite (`cargo test --workspace --features embedded-test-db`) |
| `make test-unit` | Lib tests only — use during agent runs (~5–15s warm) |
| `make test-smoke` | Lib + smoke integration (`health`, `integration_comments`, `integration_tickets`) |
| `make test-pg-reset` | Clear shared embedded Postgres session file |
| `make clippy` | `cargo clippy --workspace -- -D warnings` |
| `make clean` | `cargo clean` — remove `target/` build cache |
| `make e2e-smoke-m06` | Context long-running smoke (`continued` + pending splits) |
| `make e2e-smoke-m06-knowledge` | Governed knowledge lifecycle, retrieval, audit, extraction, and web-route smoke |
| `make benchmark-m06-knowledge-retrieval` | Default-Compose 10,000-row retrieval benchmark; asserts p95 below 250 ms |
| `make release-tar` | Self-contained release tarball |

### Context long-running tasks

Agents can return `status: "continued"` to checkpoint progress without leaving **In Progress** — the run succeeds and the next run picks up via resume context in `.agent/context.md`. PM agents may propose `splitTickets`; with default `auto_split = false` these appear as a **pending split recommendation** on the parent ticket until a human approves. See [context long-running design](superpowers/specs/2026-06-10-context-long-running-tasks-design.md).

### Knowledge configuration

M06 settings live under `[knowledge]` in TOML. The default stack uses deterministic mock embedding and extraction providers. For an OpenAI-compatible embedding endpoint, set `knowledge.embedding.provider = "openai_compatible"`, configure `base_url`, `model`, and `api_key`, and keep `dimension = 1536`. Startup fails if the configured dimension differs from the migrated `vector(1536)` column; vectors are never padded or truncated.

Knowledge embedding and extraction run on the dedicated `knowledge_jobs` queue. `knowledge.worker_count = 0` disables processing but leaves API reads available. Keep production limits in `knowledge.retrieval` and `knowledge.context_budget`; list endpoints and retrieval also enforce hard server caps.

## Disk usage / cleanup

Rust `target/` can grow to **8–16+ GB** during development (debug builds, many integration test binaries, heavy deps like sqlx/tokio/axum). It is gitignored and safe to delete.

| Command | When |
|---------|------|
| `make clean` | After a full test pass when you are done with the task |
| `cargo clean` | Same |

Do **not** run `clean` before every incremental `cargo test` — the next build will recompile everything. **Agents:** run `make clean` once after your task’s workspace tests pass.

Cursor’s agent sandbox may also cache builds under a separate `cargo-target` directory in the system temp folder. That cache is outside the repo; delete it manually if disk is tight (see Cursor docs / your temp dir).

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
