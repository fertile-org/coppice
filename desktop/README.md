# Coppice Desktop

Electron shell around the bundled Coppice server. **Setup, release, and install**: [docs/development.md](../docs/development.md).

```bash
make desktop
```

## Modes

- **Dev** — `COPPICE_WEB_URL` set (what `make desktop` does): loads that URL and spawns nothing.
- **Bundled** — `COPPICE_WEB_URL` unset: shows a splash, starts `resources/bin/coppice-server desktop` (from
  `process.resourcesPath` when packaged), loads the URL it prints on `COPPICE_READY`, and stops it on quit.
  Startup failures show an error window with the last log lines. Logs live in `<userData>/logs/`
  (`server.log`, rotated at 10 MB × 3, plus the server's `postgres.log`).

To run the bundled mode from a checkout, populate `desktop/resources/` (gitignored):

```
resources/
  bin/coppice-server            # cargo build --release -p coppice-server
  postgres/{bin,lib,share}      # yarn fetch-postgres --target <triple> --out resources/postgres
  web/                          # web/dist after `yarn build`
  agent-templates/              # server/agent_templates
  fixtures/agent-responses/     # fixtures/agent-responses
```

Then `yarn start`. Set `COPPICE_DESKTOP_USER_DATA=/tmp/coppice-test` to use a throwaway data directory
instead of the OS default (`~/.config/coppice-desktop`, `~/Library/Application Support/coppice-desktop`).

## Tests

```bash
yarn test
```
