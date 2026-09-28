# Operations reference

Compose for agents/CI, configuration detail, knowledge embedding modes, and Makefile catalog. For dev setup, release, and end-user install see [development.md](development.md).

## Configuration

Coppice uses TOML config files — not `.env` files.

| Location | Priority | Purpose |
|----------|----------|---------|
| Built-in defaults | lowest | `coppice-config` |
| `~/.config/coppice/config.toml` | middle | Per-user global (host installs) |
| `./config.toml` (cwd) | higher | Host dev (from root `config.example.toml`) |
| `deploy/config/config.toml` | Docker | Compose bind-mount |
| `COPPICE_CONFIG` | higher | Explicit file path |
| Environment variables | highest | `DATABASE_URL`, `COPPICE_*`, … |

`make compose-up` creates `deploy/config/config.toml` when missing. After editing it: `docker compose -f deploy/docker-compose.yml up -d --force-recreate server`.

### Agent stack vs human `config.toml`

| | `make compose-up` | `compose-local-up` + host API |
|--|--|--|
| Postgres port | 5432 | 5433 |
| Embedder | Ollama `:11434` (profile `embeddings`) | Same sidecar on host `:11434` |
| API | Docker `:5000` | Host `:5000` |
| Config | `deploy/config/config.toml` | `./config.toml` |
| Migrations | Auto on container start | `make migrate` |

Host `config.toml` does not affect the Docker server. Do not run `make migrate` against the wrong port.

### Auth / desktop mode

| Setting | Purpose |
|---------|---------|
| `auth.desktop_mode` | When `true`, SPA auto-calls `POST /api/auth/desktop-session` and hides login / account chrome / Users nav. Keep `false` for multi-user cloud. |
| `auth.bootstrap_admin_email` / `password` | Create first admin on empty DB (required for desktop auto-session). |

Login APIs remain available for tools and future cloud hosting.

### Repositories (desktop)

- Electron shell exposes **Browse…** for local checkouts (`window.coppiceDesktop.pickDirectory`).
- Pull / push / fetch use the checkout’s configured `origin` (SSH agent or credential helper). A stored forge token is optional and only used when present (legacy / API Create PR).
- Ticket **Open compare URL** is the primary PR path; API Create PR stays available only when a forge token is configured.

## Default Docker stack (agents / smoke)

```bash
make compose-up
```

`make compose-up` and `make compose-local-up` enable Compose profile `embeddings` (Ollama sidecar for knowledge). Opt out: `COMPOSE_PROFILES= make compose-up` or `COMPOSE_PROFILES= make compose-local-up`. Local stack: `deploy/docker-compose.local.yml`.

### Host repos for agents

| Env / path | Meaning |
|------------|---------|
| `COPPICE_REPOS_HOST` | Host directory (default `$HOME/coppice/repos`) |
| `/repos/<name>` | Register in Settings → Repositories |
| `COPPICE_UID` / `COPPICE_GID` | API user in container (`make compose-up` sets from `id -u` / `id -g`) |

Connectors: [docs/providers/](providers/README.md), [M08](milestones/M08-connector-operator-cli.md).

### Host port overrides

```bash
COPPICE_PG_PORT=55432 COPPICE_SERVER_PORT=15000 COPPICE_WEB_PORT=15001 \
COPPICE_API_URL=http://localhost:15000 COPPICE_WEB_URL=http://localhost:15001 \
  make e2e-smoke
```

## Knowledge configuration

M06 settings under `[knowledge]` in TOML.

| Mode | When | Notes |
|------|------|-------|
| **`mock`** | `make test`, CI smoke | Deterministic; no network |
| **Local sidecar** | Default `make compose-up` / `compose-local-up` + host `config.example.toml` | `openai_compatible` → `http://127.0.0.1:11434/v1` or `http://embedder:11434/v1` in Docker; `nomic-embed-text`, dim 768 |
| **Remote `openai_compatible`** | Opt-in | Your `base_url`, `model`, `api_key`, matching `dimension` |

`knowledge.embedding.dimension` must match the `vector(n)` column and provider output. Server does not wait for embedder health on boot.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` / `down` | Local Postgres (:5433) + Ollama embedder (:11434) |
| `make server-dev` | API + cargo-watch |
| `make compose-up` / `down` | Full Docker stack |
| `make migrate` / `bootstrap` | Host CLI (reads `./config.toml`) |
| `make web-dev` / `web-build` | Vite dev / production build |
| `make desktop` / `desktop-test` | Electron shell (needs web on :5001) / shell smoke |
| `make test` / `test-unit` / `test-smoke` | Rust tests |
| `make clippy` / `make clean` | Lint / reclaim `target/` |
| `make release-tar` | Release tarball → `dist/` |
| `make e2e-smoke*` | Browser/stack smokes |

## Disk usage / cleanup

Rust `target/` can grow to 10+ GB. After a **successful** full test pass for a task: `make clean`. Do not run `clean` before every incremental `cargo test`.

## CLI commands

```bash
coppice migrate
coppice health
coppice health --check-database
coppice bootstrap admin --email <email> --password <password>
coppice server start
coppice web start
```
