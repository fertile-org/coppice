# Development, release, and install

Single guide for day-to-day development, shipping versions, and how end users get Coppice. Deeper operator topics (Compose for agents, knowledge modes, Makefile catalog) live in [operations.md](operations.md). Testing: [testing.md](testing.md).

---

## Local development

### Dependencies

| Tool | Purpose |
|------|---------|
| Rust (stable) + `cargo` | API, CLI |
| `cargo-nextest` | Parallel `make test` / `make test-smoke`; without it they fall back to the slow serial runner |
| `cargo-watch` | `make server-dev` hot reload |

Install the cargo tools with `make tools` (`cargo install --locked cargo-nextest cargo-watch`; prebuilt nextest binaries: [nexte.st](https://nexte.st/docs/installation/pre-built-binaries/)).
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
- Upgrading an old volume: Postgres runs on plain `postgres:16` (no pgvector). A volume last migrated before migration 027 must be migrated once on the old `pgvector/pgvector:pg16` image (start the current server against it) before switching to `postgres:16`; migration 034 then drops the leftover `vector` extension.

Agents and CI use this path only — see [AGENTS.md](../AGENTS.md) and [operations.md](operations.md).

### Marketing screenshots

```bash
make screenshot
```

Regenerates the marketing stills and a looping GIF. The board frame is [`static/screenshot.png`](../static/screenshot.png) for the README and the same file at [`website/public/assets/hero-screenshot.png`](../website/public/assets/hero-screenshot.png). The other frames land in [`static/screenshots/`](../static/screenshots/). The GIF is [`static/marketing.gif`](../static/marketing.gif) and [`website/public/assets/marketing.gif`](../website/public/assets/marketing.gif).

Frame order, three seconds each: final-review diff, board, agent console, chat, plugins. The board crop stays 1440×900, the same window as the hero. The board includes a Plan Review column between Ready and In Progress.

It starts Compose project `coppice-screenshot` (its own volumes, ports 5432/5000/5001) with [`deploy/docker-compose.screenshot.yml`](../deploy/docker-compose.screenshot.yml), which forces `auth.desktop_mode` and turns off workflow auto-start. The seed builds a board, a Wait for Human Review diff, a finished Live Console transcript, two chats, and two plugins. The overlay does not enable the `mock-provider` feature, and the frames must not show it.

The capture matches the installed Electron app: no login screen, and the top bar does not show the bootstrap admin email or Sign out. Seeding does not start agent runs and does not change packaged desktop builds. The target is intentionally outside CI so pull requests are not gated on pixels. Stop a dev stack on those ports first; this project does not share its database.

Requires Docker, Node 22, and `ffmpeg`. The first run installs Playwright's Chromium under `e2e/`. On Linux, if Chromium is missing system libraries, install them once with `cd e2e && sudo yarn playwright install-deps chromium` (or the script falls back to a local Google Chrome). Re-run `make screenshot` after UI changes and commit the new PNG, stills, and GIF.

### Path C — Desktop shell (development only)

Electron window around the running web UI. **Does not** start Postgres or the API.

```bash
# Start Path A or B first, then:
make desktop
```

Default URL: `http://127.0.0.1:5001`. Override with `COPPICE_WEB_URL=...`. Smoke: `make desktop-test`.

On Linux, the dev shell sets `ELECTRON_DISABLE_SANDBOX=1` so Electron does not require a root-owned `chrome-sandbox` binary; the packaged `.deb` sets up `chrome-sandbox` (and an AppArmor profile for Ubuntu 24.04+) at install time. The shell preload (`desktop/preload.cjs`) exposes `window.coppiceDesktop.pickDirectory()` for Repositories **Browse…**.

To run the **bundled** app (own Postgres + server, no stack needed) from a checkout, see [desktop/README.md](../desktop/README.md). `make desktop-dist-dir` builds the unpacked package and `make desktop-smoke` runs the headless smoke against it.

### Plugins (M10)

Admins manage plugins in **Settings → Plugins**. Claude Code / Cursor format plugins and skills-only folders load unchanged; parts Coppice does not support yet (commands, hooks, …) are listed on each plugin card.

- **Plugin dirs.** The default dir comes from `[plugins] dir` (default `./data/plugins`). In Docker it is `/data/plugins` on the `plugin_data` volume (`COPPICE_PLUGINS__DIR`). It is created on start and cannot be removed. Add more dirs by path and order them; a plugin name found in an earlier dir shadows the same name in later dirs. Click **Rescan** after changing folders on disk.
- **Install from git.** Enter a URL (optional ref) and a target dir; the server shallow-clones it into that dir and rescans. Allowed: `https://` (no userinfo), `ssh://`, and `git@host:path`. `file://` URLs are rejected unless `[plugins] allow_file_git_urls = true` (default `false`; meant for tests and local experiments). Clones time out after `[plugins] git_timeout_secs` (default 300). **Update** re-pulls a git-installed plugin.
- **Enable and assign.** New plugins start disabled. Skills-only plugins start as **All agents**; a plugin with a local MCP server starts as **No agents**. On the plugin card, choose **All agents** or **Choose agents**. The agent form's **Skills** checkboxes edit the same assignment. Skills reach runs as `<plugin>:<skill>` through the `skill_list` / `skill_load` MCP tools. A change applies on each agent's next run.

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
