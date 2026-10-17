# Coppice Desktop

Electron shell around the bundled Coppice server. **Setup and release**: [docs/development.md](../docs/development.md).

```bash
make desktop
```

## Modes

- **Dev** — `COPPICE_WEB_URL` set (what `make desktop` does): loads that URL and spawns nothing.
- **Bundled** — `COPPICE_WEB_URL` unset: shows a splash, starts `resources/bin/coppice-server desktop` (from
  `process.resourcesPath` when packaged), loads the URL it prints on `COPPICE_READY`, and stops it on quit.
  Startup failures show an error window with the last log lines. Logs live in
  `$XDG_STATE_HOME/Coppice/logs` (or `~/.local/state/Coppice/logs`) on Linux and
  `~/Library/Logs/Coppice` on macOS (daily `coppice.*.jsonl` files, past days as
  `coppice.*.jsonl.zst`, and rotated Postgres logs `postgresql-*.log` plus
  `postgres.log`). Postgres rotates those logs by size and age. Coppice counts
  them toward the 256 MiB cap, deletes them after `retention_days`, and never
  deletes the file Postgres is writing.

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

In the desktop app, **Tools → Backup → Replace keys** writes new `session_secret` and `master_key` files, a new admin password, and new Postgres passwords for the bootstrap superuser (`pg_password`) and the application role (`pg_app_password`). Everyone is signed out. Tokens this install can still read are re-encrypted. Tokens already marked for re-entry stay that way until they are pasted again.

Each new Postgres password is written to a pending file, then applied with `ALTER ROLE`, then that file becomes the current password. If Coppice stops in either gap, the next launch tries the current password and then the pending one and finishes the swap. The same pending-file recovery applies to the encryption key, session secret, and admin password.

If `pg_app_password` is missing or unreadable at startup, Coppice generates a new password, sets it on `coppice_app` with the superuser connection, writes the file mode `0600`, and continues. The password is not logged.

## Updates

The shell checks GitHub releases in the background (prereleases included) and shows a banner. **Settings → Updates** remembers `automaticUpdates` in `update-settings.json` inside the user-data directory. Default is on.

Linux packages copy the committed public keyring `build/linux/coppice-archive-keyring.gpg` and add its apt source on install. The private key is used only by `.github/workflows/apt-repo.yml`, which publishes the signed repository after a release is published. The apt origin and the macOS feed origin are `COPPICE_UPDATE_BASE`, or `https://apt.getcoppice.dev` when that variable is empty. The macOS feed is that origin plus `/updates/`.

macOS in-place install is one flag. `scripts/dist.mjs` sets `updater-mode.json` → `macInPlaceUpdates` to `true` only when `CSC_LINK` is non-blank (Developer ID signing). The same file records `updateBase`. The app then uses `electron-updater` and `<updateBase>/updates/latest-mac.yml`. Unsigned builds leave the flag `false` and the banner opens the release page. To turn in-place install on, add the signing secrets listed in [docs/development.md](../docs/development.md#desktop-release-tag-and-publish). No further code change is required.

## Tests

```bash
yarn test               # or `make desktop-test` from the repo root
```
