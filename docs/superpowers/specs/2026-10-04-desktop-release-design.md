# Desktop release (.dmg / .deb from a git tag) — design

**Status:** approved design 2026-10-04; not implemented. Becomes milestone **M11 — Desktop release**; Security & sandbox and Role-owner agents move to M12 and M13.

## Why

Coppice is meant to be a local desktop app: the user downloads one installer, opens it, and Coppice runs with its own database and server — no Docker, no Postgres install, no terminal. Today only a dev Electron shell exists (`desktop/`, loads a URL of a stack started separately). This design turns a pushed version tag into a draft GitHub Release with installers for macOS and Linux.

## Decisions

| Topic | Decision |
|---|---|
| Targets | macOS arm64 `.dmg`, macOS x64 `.dmg`, Linux x64 `.deb`, Linux arm64 `.deb`. No Windows. |
| Release trigger | Push tag `vX.Y.Z` (or `vX.Y.Z-rc.N`) → CI builds all four → creates a **draft** release with the files → maintainer reviews and clicks Publish. |
| macOS signing | Wired in but optional: CI signs and notarizes when the Apple secrets exist; otherwise it ships unsigned and the release notes include the `xattr` workaround. Adding secrets requires no code change. |
| Architecture | The Rust server gains a `desktop` mode that owns Postgres, config, secrets, and SPA serving. Electron is a thin shell that starts one child process. |
| pgvector | Removed. Old migrations are rewritten so a fresh database needs no `vector` extension; existing databases get a checksum fix-up. |
| Agent CLIs | The user's own installed CLIs and existing logins (real `$HOME`, login-shell `PATH`). Research into more convenient approaches is tracked in `TODOS.md`. |
| Updates | No auto-update. On launch (and every 24 h) the app checks GitHub for a newer published release and shows a "new version available → Download" banner. |
| Postgres version | 16, pinned. Major-version upgrades of an existing data dir are out of scope; a mismatch is detected and reported. |

## Runtime: `coppice-server desktop`

```
coppice-server desktop --data-dir <D> --resources <R>
```

Electron passes `D` = its `userData` path (`~/Library/Application Support/Coppice` on macOS, `~/.config/Coppice` on Linux) and `R` = the app's resources directory.

### Data directory layout (`D`)

| Path | Contents |
|---|---|
| `config.toml` | Generated on first run from desktop defaults; never overwritten afterwards (user edits survive upgrades). |
| `secrets/` | Session secret and the encryption key for forge / plugin settings, generated on first run, files mode `0600`. Losing them loses encrypted secrets. |
| `pg/data` | Postgres cluster. |
| `artifacts/`, `worktrees/`, `plugins/` | Storage paths referenced by the generated config. |
| `logs/` | `server.log` (written by Electron from the child's output). |

### Resources layout (`R`)

| Path | Contents |
|---|---|
| `bin/coppice-server` | Release build of the server. |
| `postgres/{bin,lib,share}` | Pinned Postgres 16 bundle including `initdb`, `pg_ctl`, `postgres`, `pg_dump`, `psql`, and the `unaccent` contrib extension. |
| `web/` | `web/dist` (production SPA build). |

### Postgres lifecycle

1. **First run** (no `pg/data/PG_VERSION`): `initdb` with a generated superuser password stored in `secrets/`, `--auth=scram-sha-256`, UTF-8 locale `C`.
2. **Version check:** read `pg/data/PG_VERSION`; if its major differs from the bundled major, exit with a clear error naming both versions. Data is not touched.
3. **Stale lock:** if `pg/data/postmaster.pid` exists and its PID is not a live `postgres` process, remove it.
4. **Start:** pick a free port, `pg_ctl start` listening on `127.0.0.1` only (TCP; no Unix socket — macOS socket path length limits). Wait until it accepts connections.
5. **Database:** create the `coppice` database if missing, then run migrations.
6. **Stop** (see Shutdown): `pg_ctl stop -m fast`.

Backup / restore (Tools → Backup) use `postgres/bin/pg_dump` and `psql` from the bundle.

### Server behaviour in desktop mode

- Always binds `127.0.0.1` on a free port, regardless of config. `/api`, `/ws`, and `/mcp` are never reachable from the network.
- Forces `auth.desktop_mode = true` (no login screen, single admin session).
- Serves `R/web` with SPA fallback to `index.html` for unknown non-API paths, on the same origin as the API, so session cookies and CSRF work unchanged. Docker keeps nginx; static serving is enabled only in desktop mode.
- After migrations and workers start, prints exactly one line to stdout: `COPPICE_READY url=http://127.0.0.1:<port>`.
- Agent runs inherit the user's real `$HOME` and the `PATH` passed in by Electron.

### Shutdown

Triggered by SIGTERM, SIGINT, or stdin reaching EOF (the parent Electron process died). Order: stop accepting requests → drain in-flight requests and shut down OpenCode / plugin MCP servers → `pg_ctl stop -m fast` → exit 0. stdin EOF guarantees no orphaned Postgres when Electron crashes. A signal during startup suppresses the `COPPICE_READY` line and shuts down the same way.

Shutdown is bounded to fit Electron's 15 s SIGTERM→SIGKILL window: draining gets 6 s (after which Postgres is stopped anyway), `pg_ctl stop -m fast -w -t 5` falls back to `pg_ctl stop -m immediate -w -t 3`, and the runtime gets 1 s to wind down (6 + 5 + 3 + 1 = 15 s worst case).

Accepted deviation: active agent runs are not marked interrupted at shutdown. Agent CLI children die via `kill_on_drop`, and the startup sweep marks orphaned runs interrupted on the next launch.

## pgvector removal

Fresh databases must migrate on a plain Postgres 16 bundle. Embeddings were replaced by full-text search and `knowledge_embeddings` is dropped in migration 027, so the extension is dead weight.

- **Rewrite `001_init.sql`:** remove `CREATE EXTENSION IF NOT EXISTS vector`.
- **Rewrite `013_knowledge_learning.sql`:** the `knowledge_embeddings.embedding` column becomes `real[]` and the HNSW index is removed (the table is dropped by 027 anyway; 024 only touches constraints and is unchanged).
- **Checksum fix-up for existing databases:** before running the migrator, if `_sqlx_migrations` exists, update the stored checksum for versions 1 and 13 **only when it equals the known pre-rewrite checksum**, to the new checksum. Any other value is left alone so sqlx still reports genuine drift. Existing databases keep their installed `vector` extension harmlessly.
- Docker Compose switches from the pgvector image to plain `postgres:16`; the embedded test DB stops downloading pgvector (`test_embed.rs`); docs drop the pgvector requirement.

This ships as its own step before the desktop work.

## Electron shell (`desktop/`)

### Startup

1. `app.requestSingleInstanceLock()`; a second launch focuses the existing window.
2. Show a small splash window ("Starting Coppice…").
3. Resolve the login-shell `PATH`: run `$SHELL -ilc 'printf %s "$PATH"'` with a 5 s timeout; fall back to the current `PATH` plus common locations (`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`).
4. Spawn `R/bin/coppice-server desktop --data-dir <userData> --resources <R>` with piped stdin/stdout/stderr and the resolved `PATH`; append output to `<userData>/logs/server.log` (rotated at 10 MB, 3 files).
5. On the `COPPICE_READY` line, load the URL in the main window and close the splash.

### Failure window

If the child exits before ready, or no ready line arrives within 60 s, show an error window with the last 50 log lines and **Open logs folder**, **Retry**, **Quit**.

### Quit

On `before-quit`: SIGTERM the child, wait up to 15 s, then SIGKILL. macOS keeps the app alive when the last window closes (dock convention); Linux quits.

### Security

`contextIsolation: true`, `sandbox: true`, no `nodeIntegration`. Navigation is restricted to the server origin; other URLs open in the system browser via `shell.openExternal`. Preload exposes only `pickDirectory()` (existing), `appInfo()` → `{ version, platform, arch }`, and `getUpdateInfo()` → `{ version, url } | null` (the cached update check). The splash and error windows are local pages; the error window gets its own minimal preload (log details + Open logs / Retry / Quit), never the main window's bridge.

### Update banner

On launch and every 24 h, `GET https://api.github.com/repos/fertile-org/coppice/releases/latest` (unauthenticated; the repo slug is a build-time constant in `desktop/package.json` → `coppice.releaseRepo`). If its tag is a newer semver than `app.getVersion()`, show a dismissible bar: "Coppice X.Y.Z is available — Download", opening the release page externally. Network errors are ignored (requests time out after 10 s). The main process caches the latest result; the web banner reads it via `getUpdateInfo()` when it mounts, so a newer release found by the 24 h re-check appears after the next reload or relaunch. Dismissal is remembered per version (`localStorage` `coppice.dismissedUpdate`). Drafts and pre-releases are not returned by this endpoint, so users only see published stable releases.

### Dev mode

If `COPPICE_WEB_URL` is set, behave as today: load that URL, spawn nothing.

### Packaging (electron-builder)

- `extraResources`: `bin/coppice-server`, `postgres/`, `web/`.
- **macOS:** one `.dmg` per arch, `hardenedRuntime: true`, entitlements allowing the bundled Postgres and server binaries to run as child processes; all bundled executables and dylibs are signed. Signing and notarization run automatically when `CSC_LINK`, `CSC_KEY_PASSWORD`, `APPLE_API_KEY`, `APPLE_API_KEY_ID`, and `APPLE_API_ISSUER` are present; otherwise the build is unsigned.
- **Linux:** `.deb` with `Depends: git`; post-install sets `chrome-sandbox` to root-owned `4755` and installs an AppArmor profile granting `userns` so the Chromium sandbox works on Ubuntu 24.04+.
- Version comes from `desktop/package.json`, set from the tag by CI.

## Release pipeline (`.github/workflows/release.yml`)

Trigger: `push` of tags matching `v*`.

| Job | Runs on | Does |
|---|---|---|
| `prepare` | ubuntu | Validates the tag (`vX.Y.Z` or `vX.Y.Z-rc.N`); creates a draft release (pre-release for `-rc`) with generated notes since the previous tag plus a fixed install section (includes the macOS `xattr -dr com.apple.quarantine /Applications/Coppice.app` / Privacy & Security → Open Anyway step when signing secrets are absent). |
| `web` | ubuntu | `yarn install --frozen-lockfile && yarn build` in `web/`; uploads `web/dist` as a workflow artifact. |
| `build` (matrix) | `macos-15` (arm64), `macos-15-intel` (x64; GitHub's last x86_64 macOS image, available until Aug 2027), `ubuntu-22.04` (x64), `ubuntu-22.04-arm` (arm64) | `cargo build --release --locked -p coppice-server`; downloads the pinned Postgres bundle and verifies SHA-256; assembles resources; sets the version; runs electron-builder; **headless smoke** (below); uploads the installer to the draft release. |
| `finalize` | ubuntu | Downloads the four installers, uploads `SHA256SUMS`. |

**Headless smoke** (each matrix entry, against the packaged resources): run `coppice-server desktop` with a temp data dir, wait for `COPPICE_READY`, `GET /health` returns ok, `GET /` returns the SPA `index.html`, SIGTERM, assert exit 0 and no `postgres` process remains; run it a second time on the same data dir to prove restart.

Linux builds use Ubuntu 22.04 so the binaries run on older glibc: the `.deb` supports glibc 2.35+ (Ubuntu 22.04+, Debian 12+), since the server is built there and non-glibc shared libraries the Postgres bundle needs are copied from it. Caches: Rust (`Swatinem/rust-cache`), Yarn, and the Postgres bundle.

### Postgres bundle

Pinned in `desktop/postgres.lock.json`: version, and per target the download URL and SHA-256. Primary candidate: `theseus-rs/postgresql-binaries` (full distributions for macOS / Linux, x64 / arm64, with `pg_dump`, `psql`, contrib). Fallback: the zonky bundles used by `pg-embed`. The first implementation task verifies, per target, that the chosen bundle contains `initdb`, `pg_ctl`, `postgres`, `pg_dump`, `psql`, `unaccent`, and runs on Ubuntu 22.04's glibc; the lock file records the result.

### PR CI additions

- `cargo test` covers desktop mode (see Testing).
- A Linux job runs electron-builder with `--dir` (unpacked) to catch packaging breakage before a tag.

## Error handling

| Situation | Behaviour |
|---|---|
| Port in use | Never happens — both Postgres and the server pick free ports at start. |
| Stale `postmaster.pid` | Removed when the PID is not a live postgres process; then start normally. |
| Postgres major mismatch | Child exits with a clear message; Electron failure window shows it. Data untouched. |
| `initdb` failure / disk full | Child exits non-zero; failure window with log tail and Open logs folder. |
| `git` missing (macOS without Command Line Tools) | App starts; Settings → Repositories shows "git not found" with an install hint. |
| Agent CLI missing | Connectors page shows it missing (existing diagnostics). |
| Electron crash | Child sees stdin EOF and shuts down cleanly. |
| Update check offline / rate-limited | Silently skipped. |

## Testing

- **Rust unit:** data-dir layout and config generation (generated once, never overwritten), secret file modes, `PG_VERSION` comparison, stale-pid detection, ready-line format, checksum fix-up (updates only known old checksums, leaves unknown values).
- **Rust integration** (`embedded-test-db` feature): full desktop lifecycle — first run, ready, health, SIGTERM, clean stop; second run reuses data.
- **Migrations:** fresh DB migrates without pgvector; a DB carrying the old checksums for 1 and 13 is fixed up and migrates.
- **Electron:** unit tests for ready-line parsing, semver comparison for the banner, and login-shell `PATH` resolution with timeout fallback; `make desktop-test` runs against the unpacked packaged build.
- **Release:** headless smoke per target (above).
- **Manual acceptance:** install the `.dmg` on a real Mac and the `.deb` on a fresh Ubuntu 24.04; create a board, register a repo, run a mock-agent ticket; quit and confirm no `postgres` process remains; relaunch and confirm data persists.

## Docs

- New `docs/milestones/M11-desktop-release.md`; rename Security & sandbox → M12 and Role-owner agents → M13 (files, `docs/milestones/README.md`, `AGENTS.md`, cross-links).
- `docs/development.md`: replace "Desktop release" / "Desktop install" with how to cut a release (tag, review draft, publish), how to enable macOS signing (which secrets), and end-user install steps.
- `TODOS.md`: tick Phase 2 and Phase 3 items covered here.
- Drop the pgvector requirement from install docs and compose.

## Acceptance criteria

- [ ] Pushing `vX.Y.Z` produces a draft release with four installers and `SHA256SUMS`.
- [ ] Each installer opens a working Coppice on a clean machine with no Docker or Postgres installed.
- [ ] Data persists across quit, relaunch, and app upgrade.
- [ ] No Coppice or Postgres process remains after quit; nothing listens on non-loopback interfaces.
- [ ] macOS signing and notarization activate by adding secrets only.
- [ ] The update banner appears when a newer published release exists.
- [ ] Docker Compose and CI pass on plain Postgres 16 (no pgvector); existing databases migrate after the checksum fix-up.

## Out of scope

Windows; auto-update; Postgres major-version upgrade of existing data; remote database; in-app install or login of agent CLIs (see `TODOS.md`).
