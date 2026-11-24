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

Regenerates the homepage and README hero, plus the docs guide stills. The hero is an about-33-second storyboard recorded at 1440×900 and played at 1.25×: write a ticket, compare the design options, approve the plan, review the diff, and accept it. Status changes stay instant. The board stays at the left edge until a later column is needed, and then the edit cuts to a scroll that keeps every column whole. Shots hold still: there is no zoom and no drifting pan. A crossfade joins only two shots that already share framing and scale; any other join is a straight cut. The edit pulses the card that moved. Each caption stays through its step and switches at the cut. Accept is confirmed, and the hold after that is whatever the app shows before a merge. The review cap stays empty, so Board settings and the waiting chip are not in the frames. The looping GIF is 960px wide at [`static/marketing.gif`](../static/marketing.gif) (the README) and [`website/public/assets/marketing.gif`](../website/public/assets/marketing.gif). The same take is [`website/public/assets/marketing.mp4`](../website/public/assets/marketing.mp4) and [`website/public/assets/marketing.webm`](../website/public/assets/marketing.webm) at 1440×900. The poster still is [`website/public/assets/hero-screenshot.png`](../website/public/assets/hero-screenshot.png). Scene stills land in [`static/screenshots/`](../static/screenshots/). The design options in the popup are fixtures in [`e2e/screenshot/design-options.mjs`](../e2e/screenshot/design-options.mjs).

It starts Compose project `coppice-screenshot` (its own volumes) with [`deploy/docker-compose.screenshot.yml`](../deploy/docker-compose.screenshot.yml), which forces `auth.desktop_mode` and turns off workflow auto-start. Postgres, the API, and the web UI are published on `127.0.0.1` ports 5432, 5000, and 5001. The hero seed is the `acme-web` repo and a short board; the story ticket is “Add a dark mode toggle to Settings.” The overlay does not enable the `mock-provider` feature, and the frames must not show it. Docs stills are a separate pass in the same run.

The capture matches the installed Electron app: no login screen, and the top bar does not show the bootstrap admin email or Sign out. Seeding does not launch a connector, and does not change packaged desktop builds. The target is intentionally outside CI so pull requests are not gated on pixels. Stop a dev stack on those ports first; this project does not share its database.

Requires Docker, Node 22, `ffmpeg`, `tesseract`, and Python with Pillow. The first run installs Playwright's Chromium under `e2e/`. On Linux, if Chromium is missing system libraries, install them once with `cd e2e && sudo yarn playwright install-deps chromium` (or the script falls back to a local Google Chrome). If Playwright does not support your OS, `yarn playwright install chromium` fails and stops `make screenshot`. Run the steps by hand: install the video encoder with `cd e2e && PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64 yarn playwright install ffmpeg`, then run the capture scripts, which use Google Chrome. Re-run `make screenshot` after UI changes and commit the new PNG, stills, and GIF.

`make screenshot` creates a new private directory (`mktemp -d`, mode 0700, name `coppice-screenshot.*`), copies [`deploy/config/config.example.toml`](../deploy/config/config.example.toml) into it, and bind-mounts that file plus a new empty `repos` directory. Set `COPPICE_SCREENSHOT_TRANSFER=copy` when the Docker engine cannot see this machine's files. The same private directory is created and the symlink checks still run. Compose drops the host mounts, and `/repos` is an empty volume in the `coppice-screenshot` project. In copy mode the server container is created stopped, `docker compose cp` installs that private config, and the server process starts only after the copy. `.capture-dir` records `bind` or `copy`. `make screenshot-docs`, `make screenshot-articles`, and `make screenshot-down` use the recorded mode when the shell variable is unset, so `down -v` still removes the copy-mode repos volume. The default remains the bind mount. `make screenshot-articles` reuses the running stack and writes the article stills under `e2e/screenshot/out/pr-214/`. `make screenshot-stack` starts the same project and stops before the hero and docs captures. The Claude Code and Codex rows in those stills come from `e2e/screenshot/fixtures/bin/claude` and `codex`, copied into the container's `/home/coppice/.local/bin`. Those files are not installed on the host, and they are not a signed-in CLI. The private directory step refuses a symlink source or destination, and it does not reuse a previous directory. The path is recorded in `e2e/screenshot/.capture-dir` (mode 0600). `make screenshot-docs` reads that record and will not edit a symlink. `make screenshot-down` runs `down -v` and deletes the private directory. The next `make screenshot` also deletes the previous private directory before creating another. The server `HOME` stays `/home/coppice` on a fresh project volume. The stack's `SECRETS_MASTER_KEY` is the literal `coppice-screenshot-demo-only`, not a value from the host environment. The copied config sets `desktop_allowed_hosts` to the loopback API and web hosts so the passwordless desktop session does not use the example file's empty host list. The capture does not read `~/.ssh`, a keychain, a host CLI login, or `deploy/config/config.toml`. Git identities in the seed are `hero@coppice.local` / `Coppice Hero` and `screenshot@coppice.local` / `Coppice Screenshot`. The example config's `admin@localhost` and `changeme` values stay off screen: the scripts exit if that email or Sign out appears. Docs stills install `e2e/screenshot/fixtures/bin/agent` into the container as `cursor-agent`. That file prints a fixed model list. It is not a signed-in Cursor session. Seeded frames are demo data. They are not a successful Claude Code or Codex run.

The hero board is named `acme-web`. The committed stills do not add the word "Demo" on screen. A later article capture should put "Demo" in the board or repository name and should keep using this throwaway project. This note does not replace those stills.

What the current scripts actually drive, from the script source and the committed stills:

| State | Where it shows up | Limit |
| --- | --- | --- |
| Several tickets on one board | Hero seed creates Backlog, In Progress, In Review, In QA, and Done cards. [`static/screenshots/07-board.png`](../static/screenshots/07-board.png) shows Backlog and In Progress together. [`static/screenshots/06-accept.png`](../static/screenshots/06-accept.png) shows later columns. | The In Progress card is not a second ticket that updates after the first one merges. |
| A running ticket | The seed inserts an `agent_runs` row with status `running` for "Tighten spacing on the settings form". The board renders the "Agent running" dot from `hasActiveRun`. The dot is visible on `07-board.png`. | No connector process is running. `WORKFLOW_AUTO_START_RUNS` is false on this stack. |
| Branch and worktree | The hero creates a real git worktree and opens Code review. [`static/screenshots/05-diff.png`](../static/screenshots/05-diff.png) shows the Worktree control (`TICKET-…-acme-web · agent/TICK…`), base `main`, and `Compare main (602b313) … agent/TICKET-0ac6e090 (d68d780)`. | The full branch string is truncated in that control. The ticket drawer fields **Branch** and **Worktree** are not in this still. |
| Plan approval | The hero clicks **Approve plan**. [`static/screenshots/03-approve.png`](../static/screenshots/03-approve.png) is that beat. | **Skip planning** is set through the API for the supporting In Progress card so its status change is allowed. The checkbox is not opened. |
| Line comment | The hero clicks a diff gutter, types "Use the existing theme hook.", and clicks **Add comment**. `05-diff.png` shows that comment under the line. | The comment is one seeded line, not a review thread from an agent. |
| Accept, separate from merge | The hero confirms **Accept**. `06-accept.png` shows "Accepted, not merged" and a **Merge…** button on the Done card. | The merge dialog is not opened. No merge runs. |
| Automatic continuation | Not captured. | The product can show a continued console entry after a run reports continuation. Neither capture script seeds or opens that state. |
| Second ticket updates and retests after the first lands, including overlapping files | Not captured. | Code review can rebase a ticket branch (**Rebase…** in Git actions). A conflict lists the files and can offer **Ask [agent] to resolve**. Showing that needs a seeded pair of worktrees, a local merge of the first, and a rebase of the second. An agent actually resolving the conflict needs a live signed-in CLI. This capture does not do either. |

`node e2e/screenshot/capture.mjs` is the older five-frame script. `make screenshot` does not run it. It seeds a finished console transcript by writing `console.events.jsonl`. That transcript is not a live run, and it has no continuation entry.

Re-running `make screenshot` was not part of the blog foundation change. The table describes the scripts and the stills already in the tree. It is not a new pixel run.

`make screenshot-check` runs the directory tests and, when the Docker Compose CLI is installed, `docker compose config` on the merged screenshot files. That render does not start containers. A seeded capture is demo data for a guide. It is not an end-to-end success for Claude Code or Codex, and publishing the guide does not require those CLIs to be signed in. Assigning an agent is also not a substitute for **Run agent**: with `auto_start_runs` left at its default `true`, `POST /api/tickets/{id}/assign` starts a run only when the ticket already has a repository; **Save metadata** assigns before it saves a new repository, so that first save does not start one. The screenshot stack forces `auto_start_runs` off. Tell the reader to click **Run agent**.

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

Admins manage plugins in **Settings → Plugins**. Claude Code / Cursor format plugins and skills-only folders load unchanged; parts Coppice does not support yet (commands, hooks, …) are listed in each plugin's details.

- **Plugin dirs.** The default dir comes from `[plugins] dir` (default `./data/plugins`). In Docker it is `/data/plugins` on the `plugin_data` volume (`COPPICE_PLUGINS__DIR`). It is created on start and cannot be removed. Add more dirs by path and order them; a plugin name found in an earlier dir shadows the same name in later dirs. Click **Rescan** after changing folders on disk.
- **Install from git.** Enter a URL (optional ref) and a target dir; the server shallow-clones it into that dir and rescans. Allowed: `https://` (no userinfo), `ssh://`, and `git@host:path`. `file://` URLs are rejected unless `[plugins] allow_file_git_urls = true` (default `false`; meant for tests and local experiments). Clones time out after `[plugins] git_timeout_secs` (default 300). **Update** re-pulls a git-installed plugin.
- **Enable and assign.** New plugins start disabled. Skills-only plugins start as **All agents**; a plugin with a local MCP server starts as **No agents**. In the plugin details, choose **All agents** or **Choose agents**. The agent form's **Skills** checkboxes edit the same assignment. Skills reach runs as `<plugin>:<skill>` through the `skill_list` / `skill_load` MCP tools. A change applies on each agent's next run.

---

## Release (build and publish)

### Desktop release (tag and publish)

Desktop installers are built by `.github/workflows/release.yml`. Four targets: macOS arm64 and x64 `.dmg` (plus a `.zip` for in-place updates), Linux amd64 and arm64 `.deb`. No Windows.

**Cut a release** (maintainers):

1. Make sure `main` is green, then push a tag: `git tag v0.2.0 && git push origin v0.2.0`. Use `vX.Y.Z-rc.N` for a release candidate; any other tag shape fails the `prepare` job.
2. The workflow sets the app version from the tag, builds the server, web, and installers on `macos-15`, `macos-15-intel`, `ubuntu-22.04`, and `ubuntu-22.04-arm`, runs the headless smoke on each, and uploads `Coppice-<version>-mac-arm64.dmg`, `Coppice-<version>-mac-x64.dmg`, `Coppice-<version>-linux-amd64.deb`, `Coppice-<version>-linux-arm64.deb`, the macOS `.zip` files, and `SHA256SUMS` to a **draft** release (marked pre-release for `-rc` tags). It also uploads unversioned copies `Coppice-mac-arm64.dmg`, `Coppice-mac-x64.dmg`, `Coppice-linux-amd64.deb`, and `Coppice-linux-arm64.deb`. `SHA256SUMS` lists the installers (not the zips), so `releases/latest/download/<file>` stays the same from one release to the next. Notes are generated since the previous tag. The macOS arm64 build leg then adds the install section from `.github/release-notes/install.md`, keeping the unsigned `xattr` block only when `CSC_LINK` is empty. That leg already runs in the `release` environment, so it can see the secret; the `prepare` job does not read it. `<version>` is the tag without `v` (`0.1.0` or `0.1.0-rc.7`). That string is the app version and the file name. The `.deb` control `Version` is `0.1.0` for a final tag and `0.1.0~rc.7` for `v0.1.0-rc.7`, so apt sorts the final release after every release candidate. The package name is `coppice-desktop`.
3. Review the draft on GitHub and click **Publish**. Re-running the workflow for the same tag reuses the draft and replaces its files. Publishing starts `.github/workflows/apt-repo.yml`. That workflow does not run for the draft itself.

**Release candidates stay drafts.** Do not publish an `-rc` release. The apt repository and the update manifests follow the newest *published* release, prerelease or not, so a published rc reaches every stable install. Test an rc from its draft instead: `gh release download v0.2.0-rc.1 --pattern 'Coppice-*-linux-amd64.deb' --dir /tmp/coppice`, then `sudo apt install /tmp/coppice/Coppice-*.deb`. Publish only final `vX.Y.Z` tags.

**Tag a final release on a new commit.** If the final tag points at the same commit as an earlier published release, the Pages deploy reports success but keeps serving the old site (the deployment is keyed by commit). `v0.1.0` hit this on the `v0.1.0-rc.35` commit. Land at least one commit (for example a changelog entry) before tagging. To recover, push any commit to `main` and run `gh workflow run apt-repo.yml --ref main`.

Installed apps check the GitHub releases list for `fertile-org/coppice` (drafts skipped, prereleases included) on launch and every 4 hours. The check does not block startup. Network errors are ignored. **System → Updates** stores `automaticUpdates` in `update-settings.json` under the app data directory (default `true`).

Linux updates are an apt repository on GitHub Pages (suite `stable`, amd64 and arm64). The default host is `DEFAULT_UPDATE_BASE` in `desktop/src/updateEndpoints.mjs` (`https://apt.getcoppice.app`). The apt source uses that origin, and the macOS feed is `https://apt.getcoppice.app/updates/`. Set the repository variable `COPPICE_UPDATE_BASE` to move both to another origin. The value has to be an `https` origin with a host and no path; packaging fails otherwise. An empty value keeps the default. Each Pages deploy writes a `CNAME` file for that host. Packaging reads that variable; an empty value keeps the default. `.github/workflows/apt-repo.yml` builds the repository when a release is **published**, or from a manual run. It does not run when a draft is created, and it does not create or edit releases. The `.deb` postinst copies `/usr/share/keyrings/coppice-archive-keyring.gpg` and `/etc/apt/sources.list.d/coppice.list` from files the package build wrote. The keyring is the committed public file `desktop/build/linux/coppice-archive-keyring.gpg`. Packaging copies that file and does not read the private key. Linux packaging fails unless the repository variable `COPPICE_APT_KEY_FINGERPRINT` is set and that keyring holds exactly one primary key with that fingerprint. The Pages workflow fails unless the imported signing key has the same fingerprint. Pull-request CI signs with a throwaway key and does not read the variable.

On Linux, while a newer release is out, the shell polls `dpkg-query` for `coppice-desktop` every 10 seconds (`desktop/src/linuxInstall.mjs`). Once apt has installed a newer version, the banner offers **Restart to update**, which relaunches the app.

macOS in-place install uses `electron-updater` against `latest-mac.yml` on that same host (`<base>/updates/`). It is on only when the build was signed: `desktop/scripts/dist.mjs` writes `updater-mode.json` with `macInPlaceUpdates: true` when `CSC_LINK` is set, and records `updateBase` in that same file. Unsigned builds keep a download link. Turning in-place install on is that secret (plus the notarization secrets below); no application change is required. The Pages workflow writes `updates/latest-mac.yml` when the release has macOS zip assets. The marketing site is Vercel. `website.yml` does not deploy to Pages, because that deploy would replace the apt repository.

**Signing secrets live on GitHub environments**, not as repository secrets. `release.yml`'s build job uses the `release` environment (`CSC_*`, `APPLE_*`). `apt-repo.yml` uses the `github-pages` environment (`COPPICE_APT_SIGNING_KEY`, `COPPICE_APT_SIGNING_PASSPHRASE`). Without `CSC_LINK`, mac builds are unsigned and the release notes include the `xattr` workaround.

| Name | Where | Value |
|------|-------|-------|
| `CSC_LINK` | `release` environment secret | Base64 of the Developer ID Application `.p12` |
| `CSC_KEY_PASSWORD` | `release` environment secret | Password of that `.p12` |
| `APPLE_API_KEY` | `release` environment secret | Contents of the App Store Connect API key (`AuthKey_XXXX.p8`) |
| `APPLE_API_KEY_ID` | `release` environment secret | Key ID of that API key |
| `APPLE_API_ISSUER` | `release` environment secret | Issuer ID (UUID) of that API key |
| `COPPICE_APT_SIGNING_KEY` | `github-pages` environment secret | ASCII-armored private GPG key that signs the apt `Release` file |
| `COPPICE_APT_SIGNING_PASSPHRASE` | `github-pages` environment secret | Passphrase for that key. Empty if the key has none |
| `COPPICE_UPDATE_BASE` | repository variable | Optional apt and macOS feed origin. Empty uses `https://apt.getcoppice.app`. Must be an `https` origin with no path |
| `COPPICE_APT_KEY_FINGERPRINT` | repository variable | Full 40-character fingerprint of the apt signing key. Linux packaging and apt publish both require it |

The public keyring is `desktop/build/linux/coppice-archive-keyring.gpg`, not a variable. Replace that file with `gpg --export` of the same key as `COPPICE_APT_SIGNING_KEY` before the first release. The copy in git until then is a public key whose private half was not kept.

**Build locally:** `make desktop-dist-dir` (unpacked, host platform; `POSTGRES_DIR=<dir with bin/lib/share>` skips the pinned Postgres download), then `make desktop-smoke`. After that, `cd desktop && yarn dist` builds the installer for the host platform from the same assembled resources.

---

## Install

How people run Coppice without cloning the repo.

### Desktop install (end users)

Download the installer from the GitHub Release. Coppice bundles its own database and server, so there is no Docker, no Postgres, and no login screen (single admin session via `auth.desktop_mode`). Agents run on your machine with your own tools: install **Git** and the agent CLIs you plan to use (for example `claude` or `codex`) and log in to each of them first. Coppice finds them through your login shell's `PATH`.

**macOS** (Apple silicon: `mac-arm64`, Intel: `mac-x64`): open `Coppice-<version>-mac-<arch>.dmg`, drag **Coppice** to **Applications**, and launch it. Release builds are signed with Coppice's Developer ID and notarized by Apple.

**Linux** (Ubuntu 22.04+ / Debian 12+, `amd64` or `arm64`):

```bash
sudo apt install ./Coppice-<version>-linux-<arch>.deb
```

`<version>` in the file name is the git tag without `v` (for example `0.1.0`). The Debian package name is `coppice-desktop`, and the control `Version` is `0.1.0~rc.7` for that release candidate (`0.1.0` for the final tag). `dpkg` sorts `0.1.0~rc.7` before `0.1.0`, so installing the final `.deb` is an upgrade. Installing a newer `.deb` replaces `coppice-desktop`. `sudo apt remove coppice-desktop` leaves boards and settings in `~/.config/Coppice`.

Then start **Coppice** from the applications menu, or run `coppice`.

**Data:** `~/Library/Application Support/Coppice` on macOS, `~/.config/Coppice` on Linux (`config.toml`, `secrets/`, the Postgres cluster in `pg/data`). **Logs:** `~/Library/Logs/Coppice` on macOS; `$XDG_STATE_HOME/Coppice/logs` or `~/.local/state/Coppice/logs` on Linux. Uninstalling leaves them in place. The `secrets/` files encrypt forge and plugin settings; losing them loses those secrets. Verify downloads with `SHA256SUMS` from the release (`sha256sum --check --ignore-missing SHA256SUMS`, or `shasum -a 256 …` on macOS).

### Backup

Admins: **Tools** in the app (`/tools`), or `GET /api/tools/backup/export` / `POST /api/tools/backup/import`. A backup holds the database, settings, artifacts and worktrees. Settings leave out passwords, keys and saved tokens. The database dump still has workspace data, so keep backups private. The desktop app runs import as the non-superuser role `coppice_app`. If `secrets/pg_app_password` is missing or unreadable at startup, Coppice sets a new password on that role with the superuser connection and continues. After import, any token Coppice can't read is cleared and the app asks for it again. **Tools → Backup → Replace keys** makes a new session secret, encryption key, admin password, Postgres superuser password, and application role password, and signs everyone out. Changing `master_key` by hand makes saved tokens unreadable.
