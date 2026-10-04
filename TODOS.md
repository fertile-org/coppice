# Coppice — planned work (desktop & platform)

Tracking items discussed for the **Electron desktop** distribution and related platform work. Not a milestone spec; see `docs/milestones/` for shipped acceptance criteria.

## Desktop app (Electron)

- [ ] **Phase 1 — Dev shell**: `desktop/` runs Electron against local web URL while stack runs separately. See [docs/development.md](docs/development.md) (Path C — Desktop shell).
- [ ] **Phase 2 — Bundled runtime**: On app start, spawn bundled **PostgreSQL 16** (same class of stack as `pg-embed` / test DB), run migrations, start `coppice-server`, serve built SPA; on quit, stop children cleanly.
- [ ] **Phase 3 — Packaging**: Per-OS installers, code signing / notarization, auto-update channel, dynamic localhost ports, single-instance lock.
- [x] **Desktop auth**: Keep server auth model for cloud/self-hosted; when `auth.desktop_mode` is on, SPA auto-establishes an admin session (no login UI / account chrome). User APIs remain for cloud later.
- [x] **Repositories desktop UX**: Electron Browse for `local_path`; pull/push use host git credentials (forge token optional); ticket PR primary path is Open compare URL.
- [ ] **Agent CLI setup research (future)**: Desktop v1 runs the user's own installed agent CLIs with their existing logins (real `$HOME`, login-shell `PATH`). Research how other desktop agent tools find, install, and authenticate CLIs (bundled CLIs, in-app install/login flows, isolated homes) and adopt a more convenient approach if one exists.
- [ ] **Remote database (future)**: Optional `database.url` to external Postgres; default remains bundled data dir under app user data.
- [ ] **Testing**: Run `make desktop-test` (Playwright Electron) for shell smoke; full stack still validated via `make test` / `make e2e-smoke`. Electron does not replace CI Docker stack.

## Backup & migration

- [x] **Tools page** (`/tools`): admin export/import archive (DB + config snapshot + artifacts + worktrees). Uses host `pg_dump` / `psql` (added to server Docker image for compose).
- [ ] **Desktop**: run import on restart or from a dedicated “maintenance mode” when server is stopped (safer than hot restore).
- [ ] **Export secrets warning**: archives contain session secrets, forge tokens (encrypted at rest in DB), and `config.toml` — treat as sensitive.

## Cloud (future)

- [ ] Multi-user hosting with normal login and roles (reuse existing user model).
- [ ] Separate from desktop single-user admin experience.
