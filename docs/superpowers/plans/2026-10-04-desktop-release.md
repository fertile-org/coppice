# Desktop Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pushing a `vX.Y.Z` tag produces a draft GitHub Release with macOS (arm64, x64) `.dmg` and Linux (x64, arm64) `.deb` installers that run Coppice with a bundled Postgres 16 and no other setup.

**Architecture:** `coppice-server desktop --data-dir D --resources R` owns the Postgres lifecycle, config and secret generation, loopback-only serving, and SPA static files, and prints one `COPPICE_READY url=…` line. The Electron app in `desktop/` is a thin shell: single-instance lock, splash, spawn one child, load its URL, stop it on quit. A tag-triggered GitHub workflow builds all four targets with electron-builder and smoke-tests each one headless.

**Tech Stack:** Rust (Axum 0.8, SQLx 0.8, tokio, tower-http `fs`), Postgres 16 binaries (`theseus-rs/postgresql-binaries`, fallback zonky), Electron 35 + electron-builder, Node 22 `node:test`, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-04-desktop-release-design.md`

## Global Constraints

- Targets: macOS arm64 `.dmg`, macOS x64 `.dmg`, Linux x64 `.deb`, Linux arm64 `.deb`. No Windows code paths beyond what compiles today.
- Postgres major version 16, pinned in `desktop/postgres.lock.json` with a SHA-256 per target.
- Desktop server binds `127.0.0.1` only, on a free port; Postgres listens on `127.0.0.1` only, no Unix socket (`unix_socket_directories = ''`).
- Ready line, exactly: `COPPICE_READY url=http://127.0.0.1:<port>` on stdout, printed once after migrations and workers start.
- Shutdown triggers: SIGTERM, SIGINT, stdin EOF. Postgres stops with `pg_ctl stop -m fast`.
- Electron: ready timeout 60 s; quit waits 15 s after SIGTERM then SIGKILL; login-shell PATH timeout 5 s; server log rotated at 10 MB, keep 3 files.
- Update check: `https://api.github.com/repos/fertile-org/coppice/releases/latest`, on launch and every 24 h, failures silent.
- Tags: `vX.Y.Z` (stable) or `vX.Y.Z-rc.N` (pre-release). Draft release, never auto-published.
- macOS signing secrets: `CSC_LINK`, `CSC_KEY_PASSWORD`, `APPLE_API_KEY`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`. Absent → unsigned build, `xattr -cr /Applications/Coppice.app` in release notes.
- Generated `config.toml` is written once and never overwritten; secrets live in `D/secrets/` with mode `0600`, never in `config.toml`.
- Docker Compose / CI paths keep working (now on plain `postgres:16`). `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test` stay green.

## Review Focus

1. **App upgrade serves stale `index.html`:** after installing a new version, Electron's HTTP cache must not serve the old `index.html` that points at removed hashed assets → `index.html` is served with `Cache-Control: no-cache` (test in Task 4).
2. **Data dir path with a space** (`~/Library/Application Support/Coppice` on every Mac): `initdb`, `pg_ctl`, logs, and config paths must work → lifecycle integration test uses a data dir containing a space (Task 7).
3. **Crash leftovers:** a `postmaster.pid` whose PID is dead, or alive but not a postgres process, must not block startup → `clear_stale_pid` tests (Task 6).
4. **Noisy shell startup files** (banners, `echo` in `.zshrc`) must not corrupt the resolved PATH → marker-delimited parsing test (Task 8).
5. **Quit before ready:** quitting during the splash must still terminate the server and its Postgres → `stop()` before ready test (Task 8).

---

### Task 1: Postgres bundle lock file and fetch script

Picks and pins the Postgres binaries. Primary source `theseus-rs/postgresql-binaries` (release assets `postgresql-<ver>-<triple>.tar.gz`); fall back to the zonky bundles only if a required file is missing.

**Files:**
- Create: `desktop/postgres.lock.json`
- Create: `desktop/scripts/fetch-postgres.mjs`
- Create: `desktop/scripts/verify-postgres.mjs`
- Create: `desktop/test/fetch-postgres.test.mjs`
- Modify: `desktop/package.json` (scripts `fetch-postgres`, `test` → `node --test test/ electron-smoke.test.mjs`)
- Modify: `.gitignore` (add `/desktop/resources/`, `/desktop/.cache/`, `/desktop/dist/`)

**Interfaces:**
- Produces: `desktop/postgres.lock.json` shape `{ "version": "16.x.y", "targets": { "<triple>": { "url": string, "sha256": string } } }` for triples `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`.
- Produces: `node desktop/scripts/fetch-postgres.mjs --target <triple> --out <dir>` → `<dir>/{bin,lib,share}`; exits non-zero on checksum mismatch. Exports `verifySha256(file: string, expected: string): Promise<void>` and `targetForHost(platform: string, arch: string): string`.
- Produces: `node desktop/scripts/verify-postgres.mjs <dir>` → exits 0 when `bin/{initdb,pg_ctl,postgres,pg_dump,psql}` exist, `share/extension/unaccent.control` exists, and `bin/postgres --version` reports major 16.

- [ ] **Step 1: Write failing tests** in `desktop/test/fetch-postgres.test.mjs`: `verifySha256` resolves for a temp file whose hash matches and rejects with a message containing `checksum mismatch` otherwise; `targetForHost('darwin','arm64') === 'aarch64-apple-darwin'`, `('darwin','x64') → 'x86_64-apple-darwin'`, `('linux','x64') → 'x86_64-unknown-linux-gnu'`, `('linux','arm64') → 'aarch64-unknown-linux-gnu'`, unknown throws.
- [ ] **Step 2: Run** `cd desktop && node --test test/` — expect FAIL (module not found).
- [ ] **Step 3: Implement** both scripts (download via `fetch`, cache archive in `desktop/.cache/postgres/`, `tar -xzf` with `--strip-components=1`). Download all four theseus archives for the latest 16.x, record URLs and SHA-256 in the lock file.
- [ ] **Step 4: Verify the bundle** — `node scripts/fetch-postgres.mjs --target x86_64-unknown-linux-gnu --out /tmp/pg && node scripts/verify-postgres.mjs /tmp/pg` → exit 0. If `unaccent` or `pg_dump` is missing, switch the lock file to zonky and re-run. Record the outcome in the lock file's `"source"` field.
- [ ] **Step 5: Run** `node --test test/` — expect PASS.
- [ ] **Step 6: Commit** `feat(desktop): pinned postgres bundle and fetch script`.

### Task 2: Remove the pgvector dependency

**Files:**
- Modify: `server/migrations/001_init.sql` (drop line 1 `CREATE EXTENSION IF NOT EXISTS vector;`)
- Modify: `server/migrations/013_knowledge_learning.sql:92,96-97` (`embedding vector(1536)` → `embedding real[]`; delete the HNSW index statement)
- Create: `server/src/db/checksum_fixup.rs`
- Modify: `server/src/db/mod.rs`, `server/src/db/pool.rs` (`migrate_pool` calls the fix-up before `MIGRATOR.run`)
- Modify: `server/src/db/test_embed.rs` (remove `PGVECTOR_RELEASE`, `pgvector_target_triple`, `ensure_pgvector_extension`, extension staging)
- Modify: `deploy/docker-compose.yml:18`, `deploy/docker-compose.local.yml:10` (`pgvector/pgvector:pg16` → `postgres:16`)
- Modify: `docs/development.md`, `docs/architecture.md`, `docs/testing.md` (drop pgvector requirement wording)

**Interfaces:**
- Produces: `pub(crate) async fn fix_rewritten_migration_checksums(pool: &PgPool) -> anyhow::Result<u64>` (rows updated). Constant `REWRITTEN_MIGRATIONS: &[(i64, &str)]` = `(1, "2fef5c46864881be63c0a3fba552c77843ec9b005138d8b90a3830bcb70186d76c065d5148fcacc092cd4ca1c8d73264")`, `(13, "a161959585092fbf655fe12673fd85b62e70eb1d4570701cde655aa3ac571515495282d92322c75e7fb22299367157e9")` (SHA-384 hex of the pre-rewrite files). New checksums come from `MIGRATOR.iter()` at runtime. No-op when `_sqlx_migrations` does not exist.

- [ ] **Step 1: Pin the hashing assumption** — before editing SQL, add test `old_checksum_constants_match_current_files` asserting `MIGRATOR`'s checksum for versions 1 and 13 hex-encodes to the constants. Run `cargo test -p coppice-server --features embedded-test-db old_checksum_constants` — expect PASS (proves sqlx hashes raw file bytes). Then delete this test (it fails by design after the rewrite).
- [ ] **Step 2: Write failing tests** in `checksum_fixup.rs` (`#[cfg(all(test, feature = "embedded-test-db"))]`, using `shared_test_pool()`):
  - `fixup_rewrites_known_old_checksums`: `UPDATE _sqlx_migrations SET checksum = decode(<old v1>, 'hex') WHERE version = 1` (same for 13); `migrate_pool(&pool)` returns Ok; stored checksums now equal `MIGRATOR`'s.
  - `fixup_leaves_unknown_checksums`: set v1 checksum to `decode(repeat('00', 48), 'hex')`; `migrate_pool` returns Err (sqlx version mismatch); checksum still all zeros.
  - `fresh_database_has_no_vector_extension`: `SELECT count(*) FROM pg_extension WHERE extname = 'vector'` = 0.
- [ ] **Step 3: Run** `cargo test -p coppice-server --features embedded-test-db checksum_fixup` — expect FAIL.
- [ ] **Step 4: Rewrite the two migrations, implement the fix-up, remove pgvector from `test_embed.rs`, switch compose images, update docs.**
- [ ] **Step 5: Run** the same tests — expect PASS; then `make test-smoke`.
- [ ] **Step 6: Verify Docker path** — keep the existing compose Postgres volume (created under the pgvector image, so it carries the old checksums) and run `make compose-up && make e2e-smoke` → passes. This exercises the fix-up on a real old database running on plain `postgres:16`.
- [ ] **Step 7: Commit** `feat(db): drop pgvector dependency with checksum fix-up`.

### Task 3: Packaged-runtime robustness (resource dirs, missing git)

Packaged builds have no source tree, so the two production uses of `CARGO_MANIFEST_DIR` must be overridable. Desktop users may lack `git` (macOS without Command Line Tools), which today surfaces as a raw `No such file or directory (os error 2)`.

**Files:**
- Modify: `server/src/agent_templates/mod.rs:16-18`
- Modify: `server/src/providers/mod.rs:252-254`
- Modify: `server/src/services/repo_verifier.rs:56-59` (the `Err(err)` arm)
- Test: same files' test modules

**Interfaces:**
- Produces: `templates_dir()` returns `$COPPICE_AGENT_TEMPLATES_DIR` when set and non-empty, else the current compile-time path. `fixtures_root()` returns `$COPPICE_MOCK_FIXTURES_DIR` when set and non-empty, else the current path. Extract pure helpers `templates_dir_from(env: Option<&str>) -> PathBuf` and `fixtures_root_from(env: Option<&str>) -> PathBuf` so tests do not mutate process env.
- Produces: `pub(crate) fn git_spawn_error_message(err: &std::io::Error) -> String` — for `ErrorKind::NotFound` returns exactly `git not found — install Git (macOS: run xcode-select --install)`; otherwise `err.to_string()`. Used by the `Err(err)` arm of `verify_local_path` (shown on Settings → Repositories).

- [ ] **Step 1: Write failing tests**: `templates_dir_from(Some("/x/t")) == PathBuf::from("/x/t")`; `templates_dir_from(Some(""))` and `(None)` end with `agent_templates`; same pattern for `fixtures_root_from` ending with `fixtures/agent-responses`; `git_spawn_error_message(&io::Error::from(io::ErrorKind::NotFound))` equals the exact string above; `PermissionDenied` returns its `to_string()`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --lib templates_dir_from fixtures_root_from git_spawn_error_message` — expect FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(server): resource dir overrides and clear git-missing error`.

### Task 4: Extract `serve()` and add SPA static serving

**Files:**
- Create: `server/src/serve.rs` (startup body moved from `main.rs::run` after config load: DB connect, bootstrap, AppState, workers, `axum::serve` with graceful shutdown)
- Create: `server/src/static_web.rs`
- Modify: `server/src/main.rs` (keeps logging + `augment_process_path` + `log_directives`; calls `serve`)
- Modify: `server/src/lib.rs` (`pub mod serve; pub mod static_web;`)
- Modify: `server/Cargo.toml` (`tower-http = { version = "0.6", features = ["fs", "set-header"] }`)
- Test: `server/src/static_web.rs` tests

**Interfaces:**
- Produces: `pub struct ServeOptions { pub static_web_dir: Option<PathBuf>, pub on_ready: Option<Box<dyn FnOnce(SocketAddr) + Send>> }` (`Default`).
- Produces: `pub async fn serve(config: AppConfig, listener: tokio::net::TcpListener, options: ServeOptions, shutdown: impl Future<Output = ()> + Send + 'static) -> anyhow::Result<()>`. Calls `on_ready(listener.local_addr())` after workers are spawned and before `axum::serve`. Graceful shutdown awaits `shutdown`, then `opencode_runs.shutdown_all()` and `plugin_mcp.shutdown_all()` (as today).
- Produces: `pub fn with_static_web(router: Router, web_dir: &Path) -> Router` — fallback serves files from `web_dir`; unmatched paths that do not start with `/api`, `/mcp`, `/ws`, `/health` get `index.html`; those prefixes get 404. `index.html` responses carry `Cache-Control: no-cache`.
- `main.rs` behaviour unchanged: binds `0.0.0.0:{config.server.port}`, shutdown on ctrl-c, no static dir.

- [ ] **Step 1: Write failing tests** (tower `oneshot` on `with_static_web(Router::new(), tmp)` where `tmp` has `index.html` = `<html>app</html>` and `assets/a.js`):
  - `GET /` → 200, body contains `app`, header `cache-control: no-cache`.
  - `GET /boards/123` → 200 `index.html`.
  - `GET /assets/a.js` → 200 file body.
  - `GET /api/nope` and `GET /mcp/nope` → 404.
- [ ] **Step 2: Run** `cargo test -p coppice-server --lib static_web` — expect FAIL.
- [ ] **Step 3: Implement `static_web.rs`; move startup into `serve.rs`; slim `main.rs`.**
- [ ] **Step 4: Run** static tests — PASS; `make test-smoke` — PASS; `cargo clippy --workspace -- -D warnings` — clean.
- [ ] **Step 5: Commit** `refactor(server): extract serve() and add SPA static serving`.

### Task 5: Desktop data layout, secrets, and config generation

**Files:**
- Create: `server/src/desktop/mod.rs` (module root; `pub mod layout; pub mod bootstrap;` for now)
- Create: `server/src/desktop/layout.rs`
- Create: `server/src/desktop/bootstrap.rs`
- Modify: `config/src/lib.rs` (add `load_file_only`)
- Modify: `server/src/lib.rs` (`#[cfg(unix)] pub mod desktop;`)

**Interfaces:**
- Produces (config crate): `impl AppConfig { pub fn load_file_only(path: &Path) -> Result<Self, Box<figment::Error>> }` — defaults → that file; ignores global/local config files and env.
- Produces: `pub struct DataLayout { pub root, pub config_file, pub secrets_dir, pub pg_data, pub pg_log, pub artifacts, pub worktrees, pub plugins, pub builtin_plugins, pub logs: PathBuf }` with `DataLayout::new(root: &Path) -> Self` (`config.toml`, `secrets/`, `pg/data`, `logs/postgres.log`, `artifacts/`, `worktrees/`, `plugins/`, `builtin-plugins/`, `logs/`) and `fn ensure_dirs(&self) -> std::io::Result<()>`.
- Produces: `pub struct ResourceLayout { pub root, pub pg_bin, pub pg_lib, pub web, pub agent_templates, pub mock_fixtures: PathBuf }` with `ResourceLayout::new(root: &Path) -> Self` (`postgres/bin`, `postgres/lib`, `web`, `agent-templates`, `fixtures/agent-responses`) and `fn validate(&self) -> Result<(), String>` (error names the first missing path among `pg_bin/initdb`, `pg_bin/pg_ctl`, `pg_bin/postgres`, `web/index.html`, `agent_templates`).
- Produces: `pub struct DesktopSecrets { pub session_secret, pub master_key, pub pg_password, pub admin_password: String }`; `pub fn load_or_create_secrets(dir: &Path) -> std::io::Result<DesktopSecrets>` — one file per field (`session_secret`, `master_key`, `pg_password`, `admin_password`), 32 random bytes hex, mode `0600`, existing files reused.
- Produces: `pub fn ensure_config_file(layout: &DataLayout) -> std::io::Result<bool>` (true when created). Generated TOML sets `storage.artifacts_dir`, `agent.worktrees_path`, `plugins.dir`, `mcp.builtin_plugins_dir` to absolute layout paths, `agent.default_connector = "mock"`, a header comment saying secrets live in `secrets/`.
- Produces: `pub fn desktop_config(layout: &DataLayout, secrets: &DesktopSecrets, pg_url: &str, server_port: u16) -> anyhow::Result<AppConfig>` — `load_file_only(config_file)` then forces `server.port = server_port`, `database.url = pg_url`, `auth.desktop_mode = true`, `auth.cookie_secure = false`, `auth.session_secret`, `auth.bootstrap_admin_email = Some("admin@localhost")`, `auth.bootstrap_admin_password = Some(admin_password)`, `secrets.master_key`, `mcp.base_url = None`.

- [ ] **Step 1: Write failing tests** (tempdir-based):
  - `ensure_config_file` returns true then false; second call leaves a user edit (appended line) intact.
  - `load_or_create_secrets` twice returns identical values; each file mode `& 0o777 == 0o600`; values are 64 hex chars.
  - `desktop_config` with a config file that sets `auth.desktop_mode = false` and `server.port = 1` still yields `desktop_mode == true`, `server.port == <arg>`, `database.url == <arg>`, `mcp.base_url == None`; the generated file contains no secret value (`!contents.contains(&secrets.session_secret)`).
  - `ResourceLayout::validate` on an empty dir errs mentioning `initdb`.
  - `load_file_only` ignores `COPPICE_SERVER_PORT` env (set via a file value comparison, not env mutation: assert the file's port is returned when the file sets it).
- [ ] **Step 2: Run** `cargo test -p coppice-server --lib desktop::` and `cargo test -p coppice-config load_file_only` — expect FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(desktop): data layout, secrets and generated config`.

### Task 6: Desktop Postgres lifecycle

**Files:**
- Create: `server/src/desktop/postgres.rs`
- Modify: `server/src/desktop/mod.rs` (`pub mod postgres;`, `pub fn free_loopback_port() -> std::io::Result<u16>`)

**Interfaces:**
- Consumes: `DataLayout`, `ResourceLayout` (Task 5).
- Produces: `pub struct DesktopPostgres` with:
  - `pub fn new(resources: &ResourceLayout, layout: &DataLayout) -> Self`
  - `pub async fn bundled_major(&self) -> anyhow::Result<u32>` (runs `postgres --version`)
  - `pub fn data_major(&self) -> anyhow::Result<Option<u32>>` (reads `PG_VERSION`; `None` when uninitialised)
  - `pub async fn init_if_needed(&self, password: &str) -> anyhow::Result<bool>` — `initdb -D <data> -U coppice --pwfile=<tmp 0600 file> -A scram-sha-256 -E UTF8 --locale=C`, then appends to `postgresql.conf`: `listen_addresses = '127.0.0.1'`, `unix_socket_directories = ''`.
  - `pub fn clear_stale_pid(&self) -> anyhow::Result<bool>` — removes `postmaster.pid` when its first-line PID is not alive or `ps -p <pid> -o comm=` does not contain `postgres`; returns whether it removed the file.
  - `pub async fn start(&self, port: u16) -> anyhow::Result<()>` — `pg_ctl -D <data> -l <pg_log> -o "-p <port>" -w -t 60 start`. Paths are separate argv entries; only the numeric port goes through `-o`.
  - `pub async fn ensure_database(&self, port: u16, password: &str, name: &str) -> anyhow::Result<String>` — connects to `postgres` DB via sqlx, `CREATE DATABASE` if missing, returns `postgres://coppice:<password>@127.0.0.1:<port>/<name>`.
  - `pub async fn stop(&self) -> anyhow::Result<()>` — `pg_ctl -D <data> -m fast -w stop`; Ok if not running.
  - `pub fn parse_major(version_output: &str) -> Option<u32>`
- All child commands set `LD_LIBRARY_PATH=<pg_lib>` on Linux only, and only on those commands (never process-wide).

- [ ] **Step 1: Write failing unit tests** (no Postgres needed):
  - `parse_major("postgres (PostgreSQL) 16.4") == Some(16)`, `("postgres (PostgreSQL) 17beta1") == Some(17)`, `("garbage") == None`.
  - `data_major` → `None` on empty dir, `Some(16)` with `PG_VERSION` = `"16\n"`.
  - `clear_stale_pid`: pid file with PID `999999` → removed, returns true; pid file with `std::process::id()` (alive, not postgres) → removed, returns true; no file → false.
  - `free_loopback_port()` returns non-zero and can be bound on `127.0.0.1`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --lib desktop::postgres` — expect FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(desktop): bundled postgres lifecycle`.

### Task 7: `coppice-server desktop` runtime and lifecycle test

**Files:**
- Create: `server/src/desktop/args.rs`, `server/src/desktop/shutdown.rs`
- Modify: `server/src/desktop/mod.rs` (`pub async fn run(args: DesktopArgs) -> anyhow::Result<()>`, `pub fn ready_line(addr: SocketAddr) -> String`)
- Modify: `server/src/main.rs` (dispatch before runtime build)
- Modify: `server/src/db/test_embed.rs` (`pub async fn embedded_pg_install_dir() -> anyhow::Result<PathBuf>` — the pg-embed cache dir holding `bin/`, `lib/`, `share/`)
- Create: `server/tests/integration_desktop.rs`

**Interfaces:**
- Consumes: Tasks 3–6.
- Produces: `pub struct DesktopArgs { pub data_dir: PathBuf, pub resources: PathBuf }`; `pub fn parse_desktop_args(args: &[String]) -> Result<Option<DesktopArgs>, String>` — `None` unless `args[1] == "desktop"`; requires both `--data-dir` and `--resources`.
- Produces: `pub async fn desktop_shutdown_signal()` — resolves on SIGTERM, SIGINT, or stdin EOF.
- Produces: `ready_line(addr) == format!("COPPICE_READY url=http://127.0.0.1:{}", addr.port())`.
- `main()`: if `parse_desktop_args` returns Some, before building the runtime set `COPPICE_AGENT_TEMPLATES_DIR`, `COPPICE_MOCK_FIXTURES_DIR` from `ResourceLayout`, and prepend `pg_bin` to `PATH` (so backup's `pg_dump`/`psql` resolve), then `block_on(desktop::run(args))`; on `Err` print the error to stderr and exit 1.
- `run()` order: `ResourceLayout::validate` → `ensure_dirs` → secrets → `ensure_config_file` → version check (`data_major` vs `bundled_major`; mismatch error text: `database was created by PostgreSQL <a>; this app bundles PostgreSQL <b>`) → `init_if_needed` → `clear_stale_pid` → `start(free port)` → `ensure_database(.., "coppice")` → bind `127.0.0.1:0` → `desktop_config(.., listener port)` → `serve(config, listener, ServeOptions { static_web_dir: Some(web), on_ready: print ready_line + flush }, desktop_shutdown_signal())` → `stop()` (also on serve error).

- [ ] **Step 1: Write failing unit tests**: `parse_desktop_args(["coppice-server"]) == Ok(None)`; `["x","desktop","--data-dir","/d","--resources","/r"]` → Some with both paths; missing `--resources` → Err containing `--resources`; `ready_line("127.0.0.1:5123")` exact string.
- [ ] **Step 2: Write failing integration test** `server/tests/integration_desktop.rs` (feature `embedded-test-db`): build a resources dir in a tempdir by symlinking `postgres` → `embedded_pg_install_dir()`, writing `web/index.html`, and symlinking `agent-templates` and `fixtures/agent-responses` to the repo dirs. Data dir = `<tmp>/Application Support/Coppice` (contains a space). Spawn `env!("CARGO_BIN_EXE_coppice-server") desktop …` with piped stdin/stdout; within 120 s read `COPPICE_READY url=…`; `GET <url>/health` → 200; `GET <url>/` → body equals the written index; drop stdin (EOF); child exits 0 within 30 s; `postmaster.pid` no longer exists. Run the same spawn a second time on the same data dir → ready again (data reused, `config.toml` unchanged byte-for-byte), then SIGTERM via `kill -TERM <pid>` → exit 0.
- [ ] **Step 3: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_desktop` and the unit tests — expect FAIL.
- [ ] **Step 4: Implement** args, shutdown, `run`, main dispatch, `embedded_pg_install_dir`.
- [ ] **Step 5: Run** — expect PASS. Then `cargo clippy --workspace -- -D warnings`.
- [ ] **Step 6: Commit** `feat(desktop): coppice-server desktop runtime`.

### Task 8: Electron shell — server process, PATH, splash/error, update banner

**Files:**
- Create: `desktop/src/readyLine.mjs`, `desktop/src/serverProcess.mjs`, `desktop/src/shellPath.mjs`, `desktop/src/rotatingLog.mjs`, `desktop/src/updateCheck.mjs`, `desktop/src/windows.mjs`
- Create: `desktop/static/splash.html`, `desktop/static/error.html`
- Modify: `desktop/main.mjs`, `desktop/preload.cjs`, `desktop/package.json` (`version` field, `"coppice": { "releaseRepo": "fertile-org/coppice" }`)
- Create: `desktop/test/readyLine.test.mjs`, `serverProcess.test.mjs`, `shellPath.test.mjs`, `rotatingLog.test.mjs`, `updateCheck.test.mjs`, `desktop/test/fixtures/fake-server.mjs`
- Create: `web/src/components/DesktopUpdateBanner.tsx`, `web/src/components/DesktopUpdateBanner.test.tsx`
- Modify: `web/src/components/AppShell.tsx` (render the banner at the top of the main column)

**Interfaces:**
- Produces: `parseReadyLine(line: string): string | null` — returns the URL for `COPPICE_READY url=http://127.0.0.1:<port>` (trailing whitespace allowed), else null.
- Produces: `startServer({ command, args, env, logFile, timeoutMs = 60000 }) → { ready: Promise<string>, stop(): Promise<void>, lastLines(n = 50): string[], exited: Promise<number|null> }`. Spawns `detached: true` (own process group) with `stdio: ['pipe','pipe','pipe']`; `ready` rejects on early exit or timeout with an Error whose message includes the exit code or `timed out`. `stop()`: SIGTERM, wait up to 15 s for exit, then SIGKILL; afterwards `process.kill(-pid, 'SIGKILL')` ignoring `ESRCH` (reaps leftover agent CLIs). Safe to call before ready and more than once.
- Produces: `resolveLoginShellPath({ shell = process.env.SHELL, timeoutMs = 5000, run } = {}): Promise<string>` — runs `[shell, '-ilc', "printf '__COPPICE_PATH__%s__COPPICE_PATH__' \"$PATH\""]`, extracts between markers; on failure/timeout returns current `PATH` + `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, de-duplicated, order preserved. `run` is injectable for tests.
- Produces: `createRotatingLog(file, { maxBytes = 10 * 1024 * 1024, keep = 3 }) → { write(chunk), close() }` (`server.log` → `server.log.1` … `.3`).
- Produces: `isNewer(latestTag: string, current: string): boolean` (strips leading `v`; stable > rc of same version; numeric compare); `checkForUpdate({ repo, currentVersion, fetchImpl = fetch }): Promise<{ version: string, url: string } | null>` (null on any error or non-200).
- Produces (preload): `window.coppiceDesktop.appInfo(): Promise<{ version, platform, arch }>`, `window.coppiceDesktop.getUpdateInfo(): Promise<{ version, url } | null>` (alongside existing `pickDirectory`).
- Produces (web): `DesktopUpdateBanner` — renders nothing without `window.coppiceDesktop?.getUpdateInfo`; when it resolves non-null shows `Coppice {version} is available` with a `Download` link (`href=url`, `target="_blank"`) and a dismiss button (dismissal stored in `localStorage` key `coppice.dismissedUpdate` = version).
- `main.mjs` flow: if `COPPICE_WEB_URL` → dev mode as today. Else `requestSingleInstanceLock` (second instance focuses window), splash, `resolveLoginShellPath`, `startServer({ command: <R>/bin/coppice-server, args: ['desktop','--data-dir', app.getPath('userData'), '--resources', R], env: {...process.env, PATH}, logFile: <userData>/logs/server.log })`, on ready load URL in main window and close splash; on failure show error window (last 50 lines; buttons Open logs folder / Retry / Quit). `R = app.isPackaged ? process.resourcesPath : path.join(__dirname, 'resources')`. `before-quit`: `event.preventDefault()` once, `await server.stop()`, then `app.quit()`. Navigation restricted to the server origin (`will-navigate` + `setWindowOpenHandler` → `shell.openExternal`). Update check on ready and every 24 h, result cached for `getUpdateInfo`. Linux: drop `ELECTRON_DISABLE_SANDBOX` from the packaged path (kept only in the dev `start` script).

- [ ] **Step 1: Write failing Node tests:**
  - `parseReadyLine('COPPICE_READY url=http://127.0.0.1:5123\n') === 'http://127.0.0.1:5123'`; `parseReadyLine('INFO listening')` is null.
  - `serverProcess`: with `fake-server.mjs` (prints logs, then the ready line, exits 0 on SIGTERM) → `ready` resolves to its URL, `stop()` resolves and `exited` is 0; with fake mode `exit-early` (exits 3 before ready) → `ready` rejects with message containing `3`; with fake mode `never-ready` and `timeoutMs: 200` → rejects `timed out`; **`stop()` called immediately after start (before ready) → child is gone (`process.kill(pid, 0)` throws `ESRCH`)**; fake mode `ignore-term` → `stop()` still resolves after SIGKILL (use an injectable `graceMs` defaulting to 15000 so the test passes 100).
  - `shellPath`: injected `run` returning `"Welcome!\n__COPPICE_PATH__/a:/b__COPPICE_PATH__\nbye"` → `'/a:/b'`; `run` that rejects → result contains `/usr/local/bin` and the current PATH entries once each.
  - `rotatingLog`: `maxBytes: 10`, write 25 bytes in 5-byte chunks → `server.log.1` exists; never more than `keep` rotated files.
  - `updateCheck`: `isNewer('v1.2.0','1.1.9')` true; `isNewer('v1.2.0','1.2.0')` false; `isNewer('v1.2.0','1.2.0-rc.1')` true; `isNewer('v1.2.0-rc.2','1.2.0')` false; `checkForUpdate` with `fetchImpl` returning `{ ok: true, json: () => ({ tag_name: 'v9.0.0', html_url: 'u' }) }` → `{ version: '9.0.0', url: 'u' }`; rejecting fetch → null.
- [ ] **Step 2: Write failing web test** `DesktopUpdateBanner.test.tsx`: no `window.coppiceDesktop` → renders nothing; stub `getUpdateInfo` resolving `{ version: '9.0.0', url: 'https://x' }` → text `Coppice 9.0.0 is available`, link `Download` with href `https://x`; click dismiss → banner gone and `localStorage['coppice.dismissedUpdate'] === '9.0.0'`; re-render with the same version stays hidden.
- [ ] **Step 3: Run** `cd desktop && node --test test/` and `cd web && npx vitest run src/components/DesktopUpdateBanner.test.tsx` — expect FAIL.
- [ ] **Step 4: Implement** modules, static pages, `main.mjs`, preload, banner + AppShell wiring.
- [ ] **Step 5: Run** both test commands — PASS; `make web-test` — PASS.
- [ ] **Step 6: Manual check** — `cd desktop && node scripts/fetch-postgres.mjs --target <host> --out resources/postgres`, copy `target/release/coppice-server` to `resources/bin/`, `web/dist` to `resources/web`, `server/agent_templates` to `resources/agent-templates`, `fixtures/agent-responses` to `resources/fixtures/agent-responses`; `yarn start` with `COPPICE_WEB_URL` unset → splash, then the board UI; quit → `pgrep -f "pg/data"` prints nothing.
- [ ] **Step 7: Commit** `feat(desktop): electron shell starts bundled server`.

### Task 9: Packaging with electron-builder

**Files:**
- Create: `desktop/electron-builder.yml`
- Create: `desktop/scripts/assemble-resources.mjs`
- Create: `desktop/scripts/headless-smoke.mjs`
- Create: `desktop/scripts/dist.mjs`, `desktop/test/dist.test.mjs`
- Create: `desktop/build/entitlements.mac.plist`
- Create: `desktop/build/linux/after-install.sh`, `desktop/build/linux/after-remove.sh`, `desktop/build/linux/apparmor-coppice`
- Modify: `desktop/package.json` (devDependency `electron-builder`; scripts `assemble`, `dist`, `dist:dir`, `smoke:headless`)
- Modify: `.github/workflows/ci.yml` (new `desktop` job)
- Modify: `Makefile` (`desktop-dist-dir`, `desktop-smoke` targets)

**Interfaces:**
- Consumes: Task 1 scripts, Task 7 binary, Task 8 shell.
- Produces: `node scripts/assemble-resources.mjs --server-bin <path> --web-dist <path> --postgres <dir>` → `desktop/resources/{bin/coppice-server, postgres/, web/, agent-templates/, fixtures/agent-responses/}` (copies `server/agent_templates` and `fixtures/agent-responses` from the repo).
- Produces: `node scripts/headless-smoke.mjs --resources <dir>` — the spec's headless smoke: temp data dir, wait for ready (120 s), `GET /health` 200, `GET /` contains `<div id="root">`, SIGTERM, exit 0, `pg/data/postmaster.pid` gone; repeat once on the same data dir. Exits non-zero with the server's last 50 lines on failure.
- Produces: `yarn dist` → `node scripts/dist.mjs [--dir]`, which exports `builderArgs(env: Record<string, string | undefined>, { dir: boolean, platform: string }): { args: string[], env: Record<string, string> }` (`platform` defaults to `process.platform` at the call site) and spawns `electron-builder` with them. Rules: `--dir` passes through; on macOS, `APPLE_API_KEY` present → adds `-c.mac.notarize=true`; `CSC_LINK` absent → sets `CSC_IDENTITY_AUTO_DISCOVERY=false` (unsigned). Tests in `dist.test.mjs` cover all three rules.
- `electron-builder.yml`: `appId: dev.coppice.app`, `productName: Coppice`, `extraResources: [{ from: resources, to: . }]`, `mac: { target: dmg, hardenedRuntime: true, entitlements: build/entitlements.mac.plist, entitlementsInherit: build/entitlements.mac.plist, notarize: false }` (overridden by `dist.mjs`), `linux: { target: deb, category: Development, maintainer, executableName: coppice }`, `deb: { depends: [git], afterInstall: build/linux/after-install.sh, afterRemove: build/linux/after-remove.sh }`, artifact name `Coppice-${version}-${os}-${arch}.${ext}`.
- Entitlements: `com.apple.security.cs.allow-jit`, `com.apple.security.cs.allow-unsigned-executable-memory`, `com.apple.security.cs.disable-library-validation` (bundled Postgres dylibs).
- `after-install.sh`: `chown root:root` + `chmod 4755` on `/opt/Coppice/chrome-sandbox`; install `apparmor-coppice` to `/etc/apparmor.d/coppice` and `apparmor_parser -r` it when `/sys/kernel/security/apparmor` exists (profile: `abi <abi/4.0>, profile coppice /opt/Coppice/coppice flags=(unconfined) { userns, }`). `after-remove.sh` removes the profile.
- CI `desktop` job (`ubuntu-22.04`): rust-cache, `cargo build --release --locked -p coppice-server`, web build, fetch postgres for `x86_64-unknown-linux-gnu`, assemble, `yarn dist:dir`, `smoke:headless`, `make desktop-test`.

- [ ] **Step 1: Write failing tests** in `desktop/test/dist.test.mjs` for `builderArgs`: `({}, {dir:true, platform:"linux"})` → args include `--dir`, env `CSC_IDENTITY_AUTO_DISCOVERY === "false"`; `({CSC_LINK:"x", APPLE_API_KEY:"k"}, {dir:false, platform:"darwin"})` → args include `-c.mac.notarize=true`, no `CSC_IDENTITY_AUTO_DISCOVERY`. Run `node --test test/dist.test.mjs` — expect FAIL; `make desktop-dist-dir` — expect FAIL.
- [ ] **Step 2: Implement** config, scripts, build files, Makefile targets, CI job.
- [ ] **Step 3: Run** `make desktop-dist-dir && make desktop-smoke` — expect `desktop/dist/linux-unpacked/` present and smoke PASS.
- [ ] **Step 4: Run** `cd desktop && yarn dist` on Linux — expect `desktop/dist/Coppice-<v>-linux-amd64.deb`; `dpkg-deb -I` lists `Depends: git`.
- [ ] **Step 5: Commit** `feat(desktop): electron-builder packaging and PR CI job`.

### Task 10: Tag-triggered release workflow

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `desktop/scripts/release-version.mjs`, `desktop/test/release-version.test.mjs`
- Create: `.github/release-notes/install.md` (fixed install section, with a `{{MAC_UNSIGNED}}` block)

**Interfaces:**
- Produces: `node desktop/scripts/release-version.mjs <tag>` → validates, writes `desktop/package.json` version, prints `version=<X.Y.Z[-rc.N]>` and `prerelease=<true|false>` lines (for `$GITHUB_OUTPUT`). Exports `parseReleaseTag(tag: string): { version: string, prerelease: boolean }` (throws on anything else).
- Workflow jobs exactly as the spec's table: `prepare` (needs nothing; creates the draft with `gh release create <tag> --draft [--prerelease] --generate-notes --notes-file <rendered install.md>`; renders the unsigned block when `secrets.CSC_LINK == ''`), `web` (artifact `web-dist`), `build` matrix `{macos-15: aarch64-apple-darwin, macos-15-intel: x86_64-apple-darwin, ubuntu-22.04: x86_64-unknown-linux-gnu, ubuntu-22.04-arm: aarch64-unknown-linux-gnu}` (needs `prepare`, `web`; `cargo build --release --locked -p coppice-server`, fetch postgres, assemble, `release-version`, `yarn dist` (signing/notarization decided by `dist.mjs` from the secrets passed as env), headless smoke on the packaged resources (`make desktop-smoke`), `gh release upload <tag> <installer>`), `finalize` (needs `build`; download installers from the release, `sha256sum > SHA256SUMS`, upload). `permissions: contents: write`. Caches: rust-cache keyed by target, yarn, `desktop/.cache/postgres`.

- [ ] **Step 1: Write failing tests**: `parseReleaseTag('v1.2.3')` → `{ version: '1.2.3', prerelease: false }`; `('v1.2.3-rc.4')` → prerelease true; `'1.2.3'`, `'v1.2'`, `'v1.2.3-beta'` throw.
- [ ] **Step 2: Run** `cd desktop && node --test test/release-version.test.mjs` — expect FAIL.
- [ ] **Step 3: Implement** script, notes template, workflow.
- [ ] **Step 4: Run** the tests — PASS; `actionlint .github/workflows/release.yml .github/workflows/ci.yml` (install via `go install github.com/rhysd/actionlint/cmd/actionlint@latest` or `docker run rhysd/actionlint`) — no errors.
- [ ] **Step 5: Commit** `ci: tag-triggered desktop release workflow`.
- [ ] **Step 6: Live check** (after merge to main) — `git tag v0.1.0-rc.1 && git push origin v0.1.0-rc.1`; draft pre-release appears with 4 installers + `SHA256SUMS`; all matrix smokes green. Delete the draft and tag afterwards.

### Task 11: Docs, milestone renumbering, TODOS

**Files:**
- Create: `docs/milestones/M11-desktop-release.md` (goal, link to spec, the spec's acceptance criteria as checkboxes)
- Rename: `docs/milestones/M11-security-and-sandbox.md` → `M12-security-and-sandbox.md`; `M12-role-owner-agents.md` → `M13-role-owner-agents.md` (update their titles)
- Modify: `docs/milestones/README.md`, `AGENTS.md` (status / next-implement lines and links), any doc linking the old filenames (`rg -l "M11-security|M12-role"`)
- Modify: `docs/development.md` (replace "Desktop release" and "Desktop install" with: cutting a release, enabling macOS signing (the five secrets), end-user install for `.dmg` incl. unsigned `xattr` step and `.deb` via `sudo apt install ./Coppice-*.deb`)
- Modify: `desktop/README.md` (dev vs packaged run, `make desktop-dist-dir`)
- Modify: `TODOS.md` (tick Phase 1–3 items delivered; keep auto-update unticked as future)

- [ ] **Step 1: Run** `rg -n "M11-security-and-sandbox|M12-role-owner-agents" --glob '!target'` and update every hit after renaming.
- [ ] **Step 2: Write the docs.**
- [ ] **Step 3: Verify** `rg -n "M11-security-and-sandbox|M12-role-owner-agents|pgvector/pgvector" --glob '!target' --glob '!docs/superpowers/**'` → no hits.
- [ ] **Step 4: Final checks** — `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test`, `make desktop-test`; then `make clean`.
- [ ] **Step 5: Commit** `docs: M11 desktop release milestone and release guide`.

### Manual acceptance (after Task 10's live check)

- [ ] Install the arm64 `.dmg` on a Mac and the x64 `.deb` on a fresh Ubuntu 24.04: app opens to the board without login; create a board, register a repo, run a mock-agent ticket to Done.
- [ ] Quit → no `coppice-server` or `postgres` processes; relaunch → data present.
- [ ] `lsof -iTCP -sTCP:LISTEN -P | grep -i -e coppice -e postgres` shows only `127.0.0.1` listeners.
- [ ] Banner: run an installed build whose version is lower than the latest published stable release (e.g. install `0.1.0-rc.1` after publishing `0.1.0`) → banner appears; Download opens the release page in the browser.
