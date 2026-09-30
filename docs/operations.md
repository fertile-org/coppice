# Operations reference

Compose for agents/CI, configuration detail, knowledge compaction settings, and Makefile catalog. For dev setup, release, and end-user install see [development.md](development.md).

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

Local stack: `deploy/docker-compose.local.yml`.

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

Settings live under `[knowledge]` and `[knowledge.compaction]` in TOML. There is no embedding provider: retrieval is Postgres full-text search, and extraction is done by an agent.

| Key | Default | Meaning |
|-----|---------|---------|
| `knowledge.enabled` | `true` | Master switch for retrieval and the compaction scheduler |
| `knowledge.poll_interval_ms` | `500` | Scheduler tick (reconcile finished runs, continue drain cycles) |
| `knowledge.compaction.interval_secs` | `1800` | Time between scheduled compaction cycles |
| `knowledge.compaction.batch_max_tickets` | `10` | Done tickets per batch (one agent run) |
| `knowledge.compaction.batch_max_source_bytes` | `200000` | Byte budget for a batch's ticket snapshots |
| `knowledge.compaction.max_candidates_per_batch` | `20` | Candidates kept from one run; extras are dropped |
| `knowledge.compaction.max_attempts` | `3` | Failed attempts before a ticket waits for a manual Retry |

**Compaction agent.** An admin picks an existing agent on the Agents page (**Knowledge compaction** card, `PUT /api/settings/knowledge`). With no agent configured, Done tickets still queue but nothing is compacted. The agent runs read-only in a scratch directory under `WORKTREES_PATH/knowledge-compaction/`, so its connector must enforce read-only tools: `mock`, `claude-code`, or `cursor`. Other connectors are rejected when saving and fail closed if the connector changes later.

**Lifecycle.** Moving a ticket to Done queues it. Every `interval_secs`, or on **Compact now** from the Knowledge page, a drain cycle runs batches until the queue is empty or a batch fails. Candidates land in the Pending inbox under the fail-closed approval policy. Success sends no notification. A failure fans out a `knowledge_compaction_failed` notification to every user, returns the tickets to the queue with one more attempt, and shows a Retry banner. Cancel releases the tickets without counting an attempt.

Legacy `knowledge.embedding.*` and `knowledge.extraction.*` keys are ignored.

## Plugins

The default plugin dir is `[plugins] dir` (`COPPICE_PLUGINS__DIR`). In the default Docker stack it is `/data/plugins`, backed by the `plugin_data` volume, so git-installed plugins survive container rebuilds. Extra dirs are added in Settings → Plugins. Changing `[plugins] dir` repoints the default dir: plugins recorded under the old path are forgotten along with their agent assignments, and plugins in the new dir are scanned as new, disabled plugins.

Two conditions stop the server at startup (`AppState::init_plugins`): the configured `[plugins] dir` cannot be created, or its path is already registered as a non-default plugin dir (remove that dir in Settings → Plugins, or pick another path). Plugin scan and skill-loading failures are only logged.

## Makefile targets

| Target | What it does |
|--------|----------------|
| `make compose-local-up` / `down` | Local Postgres (:5433) |
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
