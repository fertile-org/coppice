# Development, release, and install

Single guide for day-to-day development, shipping versions, and how end users get Coppice. Deeper operator topics (Compose for agents, knowledge modes, Makefile catalog) live in [operations.md](operations.md). Testing: [testing.md](testing.md).

---

## Local development

### Dependencies

| Tool | Purpose |
|------|---------|
| Rust (stable) + `cargo` | API, CLI |
| `cargo-watch` | `make server-dev` hot reload (`cargo install cargo-watch`) |
| Node.js 22 + Yarn | Web SPA and desktop shell (`corepack enable` or install Yarn) |
| Docker + Compose | Postgres, embedder, and/or full stack |

### Configuration (minimal)

Coppice uses **TOML**, not `.env`.

- **Human hot reload:** `cp config.example.toml config.toml` — host API reads this; point `database.url` at local Postgres (`localhost:5433` with `make compose-local-up`). Root `config.example.toml` defaults knowledge embedding to the local Ollama sidecar on `http://127.0.0.1:11434/v1`.
- **Docker stack:** `deploy/config/config.toml` (created from `deploy/config/config.example.toml` on `make compose-up`).

See [operations.md — Configuration](operations.md#configuration) for paths, env overrides, and field reference.

### Path A — Human hot reload (recommended for UI/API work)

Postgres in Docker on **5433**; Ollama embedder on host **:11434** (`make compose-local-up`, profile `embeddings`). API and web on the host.

```bash
cp config.example.toml config.toml

make compose-local-up
make migrate

# Terminal 2
make server-dev
make bootstrap    # first time only

# Terminal 3
make web-dev
```

- API: http://localhost:5000/health  
- Web: http://localhost:5001 (default admin session when `auth.desktop_mode = true`; otherwise login `admin@localhost` / `changeme` after bootstrap)  
- Stop Postgres + embedder: `make compose-local-down`  
- First `compose-local-up` may pull `nomic-embed-text` (~274MB). Opt out: `COMPOSE_PROFILES= make compose-local-up`.

### Path B — Full stack in Docker (agents / smoke / production-like web)

```bash
make compose-up
```

- Web: http://localhost:5001 (nginx + built SPA, proxies `/api` and `/ws`)  
- API: http://localhost:5000  
- Stop: `make compose-down`

Agents and CI use this path only — see [AGENTS.md](../AGENTS.md) and [operations.md](operations.md).

### Path C — Desktop shell (development only)

Electron window around the running web UI. **Does not** start Postgres or the API.

```bash
# Start Path A or B first, then:
make desktop
```

Default URL: `http://127.0.0.1:5001`. Override with `COPPICE_WEB_URL=...`. Smoke: `make desktop-test`.

On Linux, the dev shell sets `ELECTRON_DISABLE_SANDBOX=1` so Electron does not require a root-owned `chrome-sandbox` binary. Packaged releases will use a proper sandbox setup.

Bundled desktop (installers, auto-start DB/API) is **not** implemented yet — see **Desktop release** and **Desktop install** below.

---

## Release (build and publish)

### Self-host tarball (server + web + CLI)

**Build** (from repo root):

```bash
make test
make clippy
make web-test
make release-tar
```

**Artifact:** `dist/coppice-<os>-<arch>.tar.gz` containing `coppice-server`, `coppice-cli`, `web/dist/`, `config.example.toml`, `systemd/`.

**Publish** (maintainers):

1. Tag a version in git (e.g. `v0.2.0`).
2. Run the build on each target OS/arch you support (or cross-compile where applicable).
3. Upload each `dist/coppice-*.tar.gz` to a **GitHub Release** (or your artifact store) with release notes.
4. Point users to [Install — Self-host tarball](#self-host-tarball).

The tarball does **not** include PostgreSQL; operators bring their own Postgres 16 + pgvector.

### Docker images (optional)

For teams that deploy with Compose instead of the tarball:

```bash
docker compose -f deploy/docker-compose.yml build
# Tag and push to your registry; document image tags in the release notes.
```

Smoke/CI uses the same compose file via `make compose-up` — not the tarball.

### Desktop release (build and publish)

| Stage | Status |
|-------|--------|
| Dev shell (`desktop/`, loads local URL) | Available |
| Bundled Postgres + API on app start | Planned ([TODOS.md](../TODOS.md)) |
| Installers (.dmg, .exe, .AppImage) + code signing | Planned |
| Auto-update channel | Planned |

**Build (today):** no end-user installer. Validate the shell with Path C above.

**Publish (when Phase 2–3 land — planned pipeline):**

1. `make release-tar` (or dedicated target) produces `coppice-server` + static web assets for embedding.
2. Package with **electron-builder** (or similar) per OS: bundle server binary, pg embed/runtime, and data-dir defaults.
3. CI matrix (macOS / Windows / Linux) produces signed artifacts.
4. Upload installers to **GitHub Releases** (or store CDN); version matches git tag.
5. Release notes: breaking changes, migration, embedding/knowledge limitations (no bundled Ollama — see [TODOS.md](../TODOS.md)).

Until that ships, **do not** tell end users to install via `desktop/` — direct them to [Install](#install) paths below.

---

## Install

How people run Coppice without cloning the repo.

### Desktop install (end users)

**Target experience:** download installer → open app → local Coppice runs (DB + API hidden) → no Docker, no login screen (single admin session via `auth.desktop_mode`).

**Today:** not available. Use self-host tarball or Docker below.

### Self-host tarball

**You need:** PostgreSQL 16 with **pgvector**, and a machine to run two processes (API + web proxy).

```bash
tar -xzf coppice-<os>-<arch>.tar.gz -C /opt/coppice
cd /opt/coppice

cp config.example.toml config.toml
# Edit: database.url, auth.session_secret, storage paths

./coppice-cli migrate
./coppice-cli bootstrap admin --email you@example.com --password '<strong-password>'

./coppice-cli server start    # :5000
./coppice-cli web start       # :5001 — open in browser
```

Optional: `mv coppice-cli coppice`. Config may also live in `~/.config/coppice/config.toml`.

**Production:** example systemd units in `systemd/` inside the tarball — set `WorkingDirectory` and `ExecStart`, then `systemctl enable --now coppice-server coppice-web`.

### Docker Compose

For operators comfortable with containers:

```bash
# On a server with Docker; use published images or build from this repo
cp deploy/config/config.example.toml deploy/config/config.toml
# Edit deploy/config/config.toml and compose env as needed
docker compose -f deploy/docker-compose.yml up -d
```

Open http://localhost:5001. Change default passwords and secrets before exposing to a network.

### Backup and migration

Admins: **Tools** in the app (`/tools`) or `GET /api/tools/backup/export` / `POST /api/tools/backup/import`. Archives are full-system backups (database, config snapshot, artifacts, worktrees) and are **sensitive**. Requires `pg_dump` / `psql` on the server host.
