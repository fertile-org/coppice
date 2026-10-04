# M11 — Desktop release (.dmg / .deb from a git tag)

## Goal

Ship Coppice as a desktop app: the user downloads one installer, opens it, and Coppice runs with its own database and server — no Docker, no Postgres install, no terminal. Pushing a version tag turns into a draft GitHub Release with installers for macOS and Linux that a maintainer reviews and publishes.

## Why before Security and Role-owner agents

- Coppice is meant to be a local desktop app; until it installs like one, every later milestone is only reachable by people who run the Docker stack.
- M12 (Security & sandbox) and M13 (Role-owner agents) build on the same server, so packaging it first means they ship to users the moment they land.

## Product scope

- **Server desktop mode:** `coppice-server desktop --data-dir <D> --resources <R>` owns a bundled Postgres 16 (initdb on first run, version check, stale-lock cleanup), generates `config.toml` and secrets once, binds `127.0.0.1` on free ports, serves the SPA from the same origin, forces `auth.desktop_mode`, prints `COPPICE_READY url=…`, and shuts down within Electron's 15 s window on SIGTERM, SIGINT, or stdin EOF
- **pgvector removed:** migrations 001 and 013 rewritten; existing databases get a checksum fix-up; Docker Compose uses plain `postgres:16`
- **Electron shell:** single instance, splash, failure window (log tail, Open logs folder, Retry, Quit), login-shell `PATH`, rotating `server.log`, origin-restricted navigation, update banner (GitHub latest release, every 24 h, notify only)
- **Packaging:** electron-builder `.dmg` (hardened runtime, optional signing + notarization) and `.deb` (`Depends: git`, setuid `chrome-sandbox`, AppArmor profile); bundled Postgres libraries checked for a closed dependency set
- **Release pipeline:** tag `vX.Y.Z` / `vX.Y.Z-rc.N` → draft release (pre-release for `-rc`) with four installers, headless smoke per target, and `SHA256SUMS`
- **Agent CLIs:** the user's own installed CLIs and logins (real `$HOME`, login-shell `PATH`)

## Out of scope

- Windows; auto-update; Postgres major-version upgrade of existing data; remote database
- In-app install or login of agent CLIs (research item in [TODOS.md](../../TODOS.md))

## Dependencies

- M01–M10 server, SPA, and desktop auth (`auth.desktop_mode`); the dev Electron shell in `desktop/`

## Design

[Design spec](../superpowers/specs/2026-10-04-desktop-release-design.md) — runtime, data layout, Postgres lifecycle, shutdown budget, Electron shell, packaging, release pipeline, testing. Release and install guide: [development.md](../development.md#desktop-release-tag-and-publish).

## Acceptance criteria

Delivered and covered by automated tests or PR CI:

- [x] `coppice-server desktop` first run, ready line, health, SPA serving, SIGTERM / stdin-EOF shutdown with no leftover Postgres, and restart on the same data dir (`server/tests/integration_desktop.rs`, unit tests in `server/src/desktop/`)
- [x] Fresh databases migrate on plain Postgres 16; databases carrying the old checksums for 001 and 013 are fixed up and migrate (`REWRITTEN_MIGRATIONS` / `fix_rewritten_migration_checksums` in `db-migrations/src/lib.rs`, tested by `server/src/db/checksum_fixup.rs`; `make test` on embedded plain Postgres)
- [x] Electron shell: ready-line parsing, login-shell `PATH` with timeout fallback, rotating log, navigation guard, update check and semver comparison, userData paths (`make desktop-test`); web update banner (`make web-test`)
- [x] Unpacked Linux package builds and passes the headless smoke twice on one data dir (local `make desktop-dist-dir desktop-smoke` run; a green PR CI `desktop` job is still pending)
- [x] Release workflow validates tags and renders install notes with or without the unsigned-macOS section (`release-version` / `render-notes` tests)

From the spec (need the live tag check or manual acceptance):

- [ ] Pushing `vX.Y.Z` produces a draft release with four installers and `SHA256SUMS`
- [ ] Each installer opens a working Coppice on a clean machine with no Docker or Postgres installed
- [ ] Data persists across quit, relaunch, and app upgrade
- [ ] No Coppice or Postgres process remains after quit; nothing listens on non-loopback interfaces
- [ ] macOS signing and notarization activate by adding secrets only
- [ ] The update banner appears when a newer published release exists
- [ ] Docker Compose and CI pass on plain Postgres 16 (no pgvector); existing databases migrate after the checksum fix-up — Rust CI passes; a real Docker Compose run on an existing pgvector volume is still pending

### Live tag check (after merge to `main`)

- [ ] `git tag v0.1.0-rc.1 && git push origin v0.1.0-rc.1`: a draft pre-release appears with four installers and `SHA256SUMS`, and every matrix smoke is green. Delete the draft and tag afterwards.

### Manual acceptance

- [ ] Install the arm64 `.dmg` on a Mac and the x64 `.deb` on a fresh Ubuntu 24.04: app opens to the board without login; create a board, register a repo, run a mock-agent ticket to Done.
- [ ] Quit → no `coppice-server` or `postgres` processes; relaunch → data present.
- [ ] `lsof -iTCP -sTCP:LISTEN -P | grep -i -e coppice -e postgres` shows only `127.0.0.1` listeners.
- [ ] Banner: run an installed build whose version is lower than the latest published stable release (e.g. install `0.1.0-rc.1` after publishing `0.1.0`) → banner appears; Download opens the release page in the browser.

## References

- [Implementation plan](../superpowers/plans/2026-10-04-desktop-release.md)
- [desktop/README.md](../../desktop/README.md)
- [M12 — Security & sandbox](./M12-security-and-sandbox.md)
