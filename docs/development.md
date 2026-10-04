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
| Docker + Compose | Postgres and/or full stack |

### Configuration (minimal)

Coppice uses **TOML**, not `.env`.

- **Human hot reload:** `cp config.example.toml config.toml` — host API reads this; point `database.url` at local Postgres (`localhost:5433` with `make compose-local-up`).
- **Docker stack:** `deploy/config/config.toml` (created from `deploy/config/config.example.toml` on `make compose-up`).

See [operations.md — Configuration](operations.md#configuration) for paths, env overrides, and field reference.

### Path A — Human hot reload (recommended for UI/API work)

Postgres in Docker on **5433** (`make compose-local-up`). API and web on the host.

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
- Stop Postgres: `make compose-local-down`

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

On Linux, the dev shell sets `ELECTRON_DISABLE_SANDBOX=1` so Electron does not require a root-owned `chrome-sandbox` binary; the packaged `.deb` sets up `chrome-sandbox` and an AppArmor profile at install time. The shell preload (`desktop/preload.cjs`) exposes `window.coppiceDesktop.pickDirectory()` for Repositories **Browse…**.

To run the **bundled** app (own Postgres + server, no stack needed) from a checkout, see [desktop/README.md](../desktop/README.md). `make desktop-dist-dir` builds the unpacked package and `make desktop-smoke` runs the headless smoke against it.

### Plugins (M10)

Admins manage plugins in **Settings → Plugins**. Claude Code / Cursor format plugins and skills-only folders load unchanged; parts Coppice does not support yet (commands, hooks, …) are listed on each plugin card.

- **Plugin dirs.** The default dir comes from `[plugins] dir` (default `./data/plugins`). In Docker it is `/data/plugins` on the `plugin_data` volume (`COPPICE_PLUGINS__DIR`). It is created on start and cannot be removed. Add more dirs by path and order them; a plugin name found in an earlier dir shadows the same name in later dirs. Click **Rescan** after changing folders on disk.
- **Install from git.** Enter a URL (optional ref) and a target dir; the server shallow-clones it into that dir and rescans. Allowed: `https://` (no userinfo), `ssh://`, and `git@host:path`. `file://` URLs are rejected unless `[plugins] allow_file_git_urls = true` (default `false`; meant for tests and local experiments). Clones time out after `[plugins] git_timeout_secs` (default 300). **Update** re-pulls a git-installed plugin.
- **Enable and assign.** New plugins start disabled. Enable a plugin, then tick it under **Plugins** in the agent form. Its skills reach runs as `<plugin>:<skill>` through the `skill_list` / `skill_load` MCP tools.

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

The tarball does **not** include PostgreSQL; operators bring their own Postgres 16.

### Docker images (optional)

For teams that deploy with Compose instead of the tarball:

```bash
docker compose -f deploy/docker-compose.yml build
# Tag and push to your registry; document image tags in the release notes.
```

Smoke/CI uses the same compose file via `make compose-up` — not the tarball.

### Desktop release (tag and publish)

Desktop installers are built by `.github/workflows/release.yml` ([design](superpowers/specs/2026-10-04-desktop-release-design.md)). Four targets: macOS arm64 and x64 `.dmg`, Linux amd64 and arm64 `.deb`. No Windows, no auto-update.

**Cut a release** (maintainers):

1. Make sure `main` is green, then push a tag: `git tag v0.2.0 && git push origin v0.2.0`. Use `vX.Y.Z-rc.N` for a release candidate; any other tag shape fails the `prepare` job.
2. The workflow sets the app version from the tag, builds the server, web, and installers on `macos-15`, `macos-15-intel`, `ubuntu-22.04`, and `ubuntu-22.04-arm`, runs the headless smoke on each, and uploads `Coppice-<version>-mac-arm64.dmg`, `Coppice-<version>-mac-x64.dmg`, `Coppice-<version>-linux-amd64.deb`, `Coppice-<version>-linux-arm64.deb`, and `SHA256SUMS` to a **draft** release (marked pre-release for `-rc` tags). Notes are generated since the previous tag, plus the install section from `.github/release-notes/install.md`.
3. Review the draft on GitHub and click **Publish**. Re-running the workflow for the same tag reuses the draft and replaces its files.

Installed apps check `releases/latest` of `fertile-org/coppice` on launch and every 24 h and show a "new version available" banner. Drafts and pre-releases are never offered.

**Enable macOS signing and notarization:** add these repository secrets; no code change is needed. Without `CSC_LINK`, mac builds are unsigned and the release notes include the `xattr` workaround.

| Secret | Value |
|--------|-------|
| `CSC_LINK` | Base64 of the Developer ID Application `.p12` |
| `CSC_KEY_PASSWORD` | Password of that `.p12` |
| `APPLE_API_KEY` | Contents of the App Store Connect API key (`AuthKey_XXXX.p8`) |
| `APPLE_API_KEY_ID` | Key ID of that API key |
| `APPLE_API_ISSUER` | Issuer ID (UUID) of that API key |

**Build locally:** `make desktop-dist-dir` (unpacked, host platform; `POSTGRES_DIR=<dir with bin/lib/share>` skips the pinned Postgres download), then `make desktop-smoke`. After that, `cd desktop && yarn dist` builds the installer for the host platform from the same assembled resources.

---

## Install

How people run Coppice without cloning the repo.

### Desktop install (end users)

Download the installer from the GitHub Release. Coppice bundles its own database and server, so there is no Docker, no Postgres, and no login screen (single admin session via `auth.desktop_mode`). Agents run on your machine with your own tools: install **Git** and the agent CLIs you plan to use (for example `claude` or `codex`) and log in to each of them first. Coppice finds them through your login shell's `PATH`.

**macOS** (Apple silicon: `mac-arm64`, Intel: `mac-x64`): open `Coppice-<version>-mac-<arch>.dmg`, drag **Coppice** to **Applications**, and launch it. If the build is not signed, macOS says Coppice "can't be opened" or "is damaged"; run once:

```bash
xattr -dr com.apple.quarantine /Applications/Coppice.app
```

Alternatively, after the first blocked launch open **System Settings → Privacy & Security** and click **Open Anyway**.

**Linux** (Ubuntu 22.04+ / Debian 12+, `amd64` or `arm64`):

```bash
sudo apt install ./Coppice-<version>-linux-<arch>.deb
```

Then start **Coppice** from the applications menu, or run `coppice`.

**Data and logs:** `~/Library/Application Support/Coppice` on macOS, `~/.config/Coppice` on Linux (`config.toml`, `secrets/`, the Postgres cluster in `pg/data`, `logs/server.log`). Uninstalling leaves them in place. The `secrets/` files encrypt forge and plugin settings; losing them loses those secrets. Verify downloads with `SHA256SUMS` from the release (`sha256sum --check --ignore-missing SHA256SUMS`, or `shasum -a 256 …` on macOS).

### Self-host tarball

**You need:** PostgreSQL 16 and a machine to run two processes (API + web proxy).

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

Admins: **Tools** in the app (`/tools`) or `GET /api/tools/backup/export` / `POST /api/tools/backup/import`. Archives are full-system backups (database, config snapshot, artifacts, worktrees) and are **sensitive**. Requires `pg_dump` / `psql` on the server host (the desktop app uses its bundled copies).
