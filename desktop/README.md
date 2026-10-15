# Coppice Desktop

Electron shell around the bundled Coppice server. **Setup, release, and install**: [docs/development.md](../docs/development.md).

```bash
make desktop
```

## Modes

- **Dev** — `COPPICE_WEB_URL` set (what `make desktop` does): loads that URL and spawns nothing.
- **Bundled** — `COPPICE_WEB_URL` unset: shows a splash, starts `resources/bin/coppice-server desktop` (from
  `process.resourcesPath` when packaged), loads the URL it prints on `COPPICE_READY`, and stops it on quit.
  Startup failures show an error window with the last log lines. Logs live in
  `$XDG_STATE_HOME/Coppice/logs` (or `~/.local/state/Coppice/logs`) on Linux and
  `~/Library/Logs/Coppice` on macOS (`postgres.log` and daily `coppice.*.jsonl`
  files; past days are stored as `coppice.*.jsonl.zst`).

To run the bundled mode from a checkout, populate `desktop/resources/` (gitignored):

```
resources/
  bin/coppice-server            # cargo build --release -p coppice-server
                                # (no --features mock-provider; the mock connector is test/dev only)
  postgres/{bin,lib,share}      # yarn fetch-postgres --target <triple> --out resources/postgres
  web/                          # web/dist after `yarn build`
  agent-templates/              # server/agent_templates
```

Then `yarn start`. Unpackaged runs keep their data in `Coppice-dev` (`~/.config/Coppice-dev`,
`~/Library/Application Support/Coppice-dev`), separate from an installed app's `Coppice` directory,
so the two never share a database or the single-instance lock. Set
`COPPICE_DESKTOP_USER_DATA=/tmp/coppice-test` to use a throwaway data directory instead.
This variable only applies to unpackaged runs from a checkout; packaged apps ignore it.

## Packaged build

From the repo root:

```bash
make desktop-dist-dir    # release server + web build, fetch pinned Postgres, assemble resources/, electron-builder --dir
make desktop-smoke       # headless smoke against the packaged resources (two runs on one data dir)
```

`POSTGRES_DIR=<dir with bin/lib/share>` skips the pinned download in `postgres.lock.json`
(e.g. `make desktop-dist-dir POSTGRES_DIR=$HOME/.cache/pg-embed/linux/amd64/16.12.0`). The unpacked app lands in
`dist/linux-unpacked/` or `dist/mac*/Coppice.app`; `yarn dist` then builds the `.dmg` / `.deb` for the host.
Packaged apps keep their data in `Coppice` (`~/.config/Coppice`, `~/Library/Application Support/Coppice`).
Tagged releases are built by `.github/workflows/release.yml` — see
[docs/development.md](../docs/development.md#desktop-release-tag-and-publish).

## Replace keys

In the desktop app, **Tools → Backup → Replace keys** writes new `session_secret` and `master_key` files, a new admin password, and a new Postgres password. Everyone is signed out and saved tokens are re-encrypted. A server install uses `coppice-server replace-keys --admin-password …` with Coppice stopped and PostgreSQL still running.

The new Postgres password is written to a pending file, then applied with `ALTER ROLE`, then that file becomes the current password. If Coppice stops in either gap, the next launch tries the current password and then the pending one and finishes the swap. The same pending-file recovery applies to the encryption key, session secret, and admin password.

## Tests

```bash
yarn test               # or `make desktop-test` from the repo root
```
