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
| Embedder | Ollama `:11434` (profile `embeddings`) | Same sidecar on host `:11434` |
| API | Docker `:5000` | Host `:5000` |
| Config source | `deploy/config/config.toml` + compose env | `./config.toml` on the host |
| Migrations | Server auto-migrates on container start | `make migrate` (reads `config.toml`) |

Your gitignored repo-root `config.toml` (e.g. `database.url` → `:5433`) applies only to **host** CLI and `coppice-server` when run on the host. It does not affect the Docker server. Avoid running host `make migrate` against the agent stack unless you override the URL, e.g. `DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice make migrate` — otherwise you may migrate the wrong database.

## Local development (human)

Postgres runs in Docker on port **5433**. The Ollama embedder sidecar listens on host **:11434** (Compose profile `embeddings`, Makefile default). API and web run on the host for hot reload.

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

Tear down Postgres + embedder: `make compose-local-down`. First `compose-local-up` may take several minutes while Ollama pulls `nomic-embed-text` (~274MB). Opt out: `COMPOSE_PROFILES= make compose-local-up`.

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

`make compose-up` enables Compose profile `embeddings` (Makefile default). That starts the **Ollama embedder sidecar** beside Postgres/server/web and points the server at it (`openai_compatible` → `nomic-embed-text`, dimension `768`). First boot pulls the model (~274MB, often 1–5+ minutes); weights persist in the `ollama_data` volume. The API does **not** wait on embedder health — knowledge embed jobs fail until the sidecar is ready. Opt out with `COMPOSE_PROFILES= make compose-up` (no Ollama container). Human hot reload (`make compose-local-up`) starts the same sidecar on host `:11434`; root `config.example.toml` points at `http://127.0.0.1:11434/v1`.

The **web** service is a production image: `yarn build` then **nginx** on `:5001` (static SPA, proxies `/api` and `/ws` to `server:5000`). For UI hot reload, use the human path (`make web-dev`), not Compose web.

Docker config (`deploy/config/config.toml`, from `config.example.toml` in that folder) can set `auth.bootstrap_admin_email` / `auth.bootstrap_admin_password`. Host installs without those fields still use `make bootstrap` (or `coppice bootstrap admin`) once.

Tear down: `make compose-down`

Always use Docker Compose via the Makefile — not standalone `docker run`.

### Agent dev toolchain

The default **server** image ships build/verification tools so Cursor (and other real connectors) can run Coppice checks from a ticket worktree inside the container. Vendor CLIs (`agent`, `claude`, …) are **not** baked in — install those via the managed `/home/coppice` volume per [M08](milestones/M08-connector-operator-cli.md).

| Tool | Version / source |
|------|------------------|
| `cargo` / `rustc` | 1.88 (copied from `rust:1.88-bookworm` builder stage) |
| `make` | Debian `bookworm-slim` |
| `node` / `npm` | 22 (`node:22-bookworm` stage) |
| `yarn` | 1.22.22 (corepack) |

Compose prepends `/usr/local/cargo/bin` to `PATH` (see `deploy/docker-compose.yml`) so agent child processes inherit `cargo`/`rustc` alongside connector binaries under `$HOME/.local/bin`.

From a Coppice worktree mounted or checked out inside the server container:

```bash
cargo --version && make --version && node --version && yarn --version
make test-unit
make web-test
```

**Image size** (measure after `docker compose -f deploy/docker-compose.yml build server` with `docker image inspect deploy-server --format '{{.Size}}'`; expected ranges from ticket sizing analysis):

| Image | Size |
|-------|------|
| Baseline runtime (slim Debian + Coppice binaries only) | ~200–250 MB |
| After dev toolchain (Rust + Node + build-essential) | ~800 MB–1.2 GB |

Breakdown: +500–900 MB Rust std/toolchain, +100–150 MB Node, +150–250 MB build-essential/make. Build-time delta: +1–3 min for extra COPY/apt layers; the Rust release compile in the `builder` stage is unchanged.

**Runtime caveats:**

- First `make test-unit` in a worktree compiles the workspace (`target/` under the worktree volume bind-mount).
- First run with `embedded-test-db` may download pg-embed binaries (requires outbound network; fails closed in air-gapped deploys).
- After a full verification pass, run `make clean` in the worktree to reclaim disk (see [Disk usage / cleanup](#disk-usage--cleanup)).

### Host repos for agents

The server bind-mounts host git checkouts at `/repos`:

| Env / path | Meaning |
|------------|---------|
| `COPPICE_REPOS_HOST` | Host directory (default `$HOME/coppice/repos`) |
| `/repos/<name>` | Path to register in Settings → Repositories |
| `COPPICE_UID` / `COPPICE_GID` | Host user the API runs as (default `1000`; `make compose-up` uses `id -u` / `id -g`) |

Clone or symlink projects into that host directory, then register `/repos/<name>` (the in-container path). Coppice creates worktrees under `/data/worktrees`; it does not `git clone` from `remote_url`.

The server entrypoint starts as root only long enough to `chown` `/data/*` volumes, then drops to `COPPICE_UID`/`COPPICE_GID` so Git ownership matches the bind mount and new files under `/repos` stay yours on the host. If an older root-owned run left files behind: `sudo chown -R "$(id -u):$(id -g)" ~/coppice/repos`.

To use a real connector (Cursor, OpenCode, …), follow that connector’s **One-time setup** in [docs/providers/](providers/README.md) (run `coppice connector …` on the **server** container).

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

Only the host-side mapping changes; container-internal ports and the `postgres` service DNS are unaffected, so `DATABASE_URL` inside the stack stays the same. The smoke scripts read `COPPICE_API_URL` / `COPPICE_WEB_URL` (defaults `:5000` / `:5001`), so set those to match when you move the server/web ports.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` | Start local Postgres (5433) + Ollama embedder (11434) |
| `make compose-local-down` | Stop local Postgres + embedder |
| `make server-dev` | API with `cargo watch` (hot reload) |
| `make compose-up` | Default Docker stack (agents / CI) |
| `make compose-down` | Stop default stack |
| `make migrate` | `coppice migrate` (reads `config.toml` on host) |
| `make bootstrap` | `coppice bootstrap admin` |
| `make web-dev` | Host Vite hot reload (proxies to `:5000`); Compose web uses nginx instead |
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

M06 settings live under `[knowledge]` in TOML. There are **three embedding modes** — do not confuse CI mock with the Docker install default.

| Mode | When | Provider config | Notes |
|------|------|-----------------|-------|
| **`mock`** | `make test`, knowledge unit/integration tests, e2e/CI smoke | `provider = "mock"` (Makefile forces this for smoke) | Deterministic hashed vectors; no download, no network, no GPU |
| **Local Compose sidecar** | Default after `make compose-up` or `make compose-local-up` | `openai_compatible` → `nomic-embed-text` @ `768`, placeholder `api_key`. Docker server: `http://embedder:11434/v1`. Host API: `http://127.0.0.1:11434/v1` (root `config.example.toml`) | Ollama under Compose profile `embeddings`; first boot pulls ~274MB into `ollama_data` |
| **Remote `openai_compatible`** | Opt-in (OpenAI or any `/v1/embeddings` host) | Same provider string; set your `base_url`, `model`, `api_key`, and matching `dimension` | Disable or ignore the sidecar; point env/TOML at the remote host |

**Retrieval split:** cosine ranking and HNSW live in **Postgres** (`knowledge_embeddings`). Query text still goes to the **configured embedding provider** first (mock, local Ollama, or remote). Provider downtime breaks new embeds and Full-run query embedding even though stored vectors remain in the DB.

**Dimension must match the model.** `knowledge.embedding.dimension` must equal the live `vector(n)` column and the provider’s output length. Startup requires the column type to match config; vectors are never padded or truncated. A provider response with the wrong length fails the embed job and leaves the previous active revision intact. Config/column mismatch with existing rows fails startup until embeddings are cleared and re-embedded.

**Changing dimension:** set `knowledge.embedding.dimension` to the new size. If `knowledge_embeddings` is empty, startup rewrites the column (and HNSW index) to `vector(n)`. If rows already exist at another dimension, startup fails — run `DELETE FROM knowledge_embeddings`, restart so the column can be rewritten, then re-embed (approve/re-queue revisions). Do not mix dimensions in one column.

The server does **not** wait for embedder health on Compose boot. Soft dependency: API stays up while nomic pulls or if the sidecar/profile is absent; embed jobs error until the endpoint is reachable. E2e Makefile targets clear `COMPOSE_PROFILES` and force `PROVIDER=mock` so CI stays deterministic.

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
