# Plugin Marketplace — Design

**Status:** proposed (2026-10-25). Ticket `45650670` "Plugin: marketplace". Part 3 of [M10 plugins](../../milestones/M10-plugins.md). Extends [standard skills and marketplaces](2026-10-04-standard-skills-and-marketplaces-design.md) (folder classification, `marketplace.json` expansion, external rows) and [M10 plugins](2026-09-29-m10-plugins-design.md) (gateway, plugin MCP proxy). Must not merge ahead of M12 without human sign-off.

## Goal

A browsable catalog of plugins and skills inside Coppice, so users stop installing by hand. The catalog has two kinds of source: Coppice's own first-party plugins (for example `github`, maintained in this repo) and external sources published as `marketplace.json`. Installing an entry adds one plugin to Coppice. Every agent the user grants access to gets its skills and MCP tools through the MCP gateway, whatever the connector.

## Decisions (2026-10-25)

| Topic | Decision |
|-------|----------|
| Scope | A Coppice catalog of plugins. It belongs to no connector. |
| Connector writes | None. Coppice never writes into `~/.claude`, `~/.cursor`, `~/.codex`, or the worktree. Tools reach agents only through `/mcp`. |
| Manifest format | Existing `.claude-plugin/marketplace.json`. It is a file format Coppice reads. It says nothing about which connectors can use an entry. No `.coppice-plugin/` format. |
| Discovery | Three layers: built-in Coppice source, curated external entries in the same file, user-added git sources. |
| Rejected for v1 | GitHub search (noisy, rate-limited, no trust signal), a hosted registry (ops burden, not local-first), the MCP Registry (no skills; a possible later source kind). |
| Install model | Plugin rows are derived from the filesystem, so marketplace install writes a folder, then runs `rescan`. |
| Trust | Existing gates stay: installs start disabled, stdio MCP servers need confirmation, local MCP defaults to "No agents". Cards show **Coppice** or **Community: not reviewed by Coppice**. No connector badges. |

## Sources

1. **Coppice source (built in).** Lives at `server/marketplace/coppice/` as `.claude-plugin/marketplace.json` plus `plugins/<name>/`. It is not in a repo-root `plugins/` because `deploy/Dockerfile.server` copies only `server/`, `cli/`, `connectors/`, and similar paths, and crate-relative embeds must stay inside the crate. The whole tree is embedded with `include_dir` or a `build.rs`-generated list, not one `include_str!` per file. At startup it is materialized into the marketplace cache, the same way `materialize_builtin` writes `mcp.builtin_plugins_dir`. That keeps it version-matched and offline.
2. **Curated external entries.** Object-source entries (`github`, `url`, `git-subdir`) in the same file, reviewed by PR.
3. **User-added sources.** **Add marketplace** takes a git URL that contains `marketplace.json`. It is shallow-cloned into the marketplace cache and its entries are listed without creating plugin rows. **Refresh** pulls it again.

### Cache directory

- New config key `plugins.marketplaces_dir`, default `./data/marketplaces`. Wired into `config.example.toml`, the desktop `DataLayout`, and `desktop/bootstrap.rs`, next to `builtin_plugins`.
- Layout: `<cache>/coppice/` (embedded, rewritten on every start) and `<cache>/<source_id>/` (git clones).
- **Hard rule:** the cache must not sit inside any `plugin_dirs.path`. Otherwise `PluginService::rescan` → `discover()` → `expand_marketplace` turns every catalog entry into a plugin row. Check this at startup and in `add_dir` / `ensure_default_dir`.
- The cache is regenerable, so backups exclude it.

## Install, update, and conflicts

| Entry kind | Install | Update |
|------------|---------|--------|
| Path (`./plugins/x`) | Resolve with `normalize_source` and the canonicalize-inside-root check. Copy into a temp sibling inside the **default plugin dir**, rename to `<name>`, run `rescan`, then stamp `origin_*` on the row matched by `(default_dir_id, '<name>')`. Reject symlinks that point outside the entry root. Do not follow symlinked directories. | Re-copy into a temp sibling and swap by rename, then `rescan`. |
| `github` / `url` | Reuse the async install pipeline: `start_install` → `plugin_installs` job → `finish_install` / `complete_git_job`. `POST /api/marketplace/install` returns the job, as git install does. Stamp `origin_*` in `finish_install`. Do not clone synchronously in the handler. | Same pipeline as install. |
| `git-subdir` | New job variant. Clone to a temp dir, validate `path` with the same normalize and canonicalize rules, copy only that subfolder into the default plugin dir, then follow the path rules. | Same as install. |
| `npm`, unknown | Listed with `installable: false` and a reason. | n/a |

- **Conflicts:** return 409 if `<plugins.dir>/<name>` exists, or a plugin named `<name>` exists in any dir. Otherwise the shadowing pass in `rescan` silently hides one of the two.
- **Row identity on update:** `rel_path` does not change, so the row id is preserved. Enabled state, settings (keyed by plugin id), and agent access survive.
- **Provenance columns:** `origin_source_id`, `origin_entry`, `origin_commit` on `plugins`. `upsert_discovered` must not overwrite `origin_*`, `enabled`, `agent_access`, or `disabled_skills` on conflict.
- **`updateAvailable`:** `origin_commit != source.commit_sha`, or, for the built-in source, the entry `version` differs.
- **Removing a source:** `origin_source_id` becomes NULL (`ON DELETE SET NULL`) and the cache folder is deleted. Plugins stay installed and lose only Update.
- The built-in source cannot be removed (409).
- Legacy **Install from git** on a marketplace repo is unchanged. It still creates one row per entry with `external` status. The marketplace does not use that path.

## Data model

```
marketplace_sources
  id uuid, name text, kind text ('builtin' | 'git'), git_url text, git_ref text,
  commit_sha text, refreshed_at timestamptz, last_error text, position int

plugins
  + origin_source_id uuid NULL REFERENCES marketplace_sources(id) ON DELETE SET NULL
  + origin_entry text NULL
  + origin_commit text NULL
```

Migration `054_marketplace_sources.sql`. The built-in source is seeded by the service at startup.

## API

Admin + CSRF guard, the same as `api/plugins.rs`. Handlers stay thin; logic lives in `server/src/services/marketplace_service.rs`.

| Route | Purpose |
|-------|---------|
| `GET /api/marketplace/sources` | list sources |
| `POST /api/marketplace/sources` | add a git source (uses `validate_git_url`) |
| `DELETE /api/marketplace/sources/:id` | remove; built-in returns 409 |
| `POST /api/marketplace/sources/:id/refresh` | re-clone or pull a git source, bounded by `git_timeout_secs`, never holding `lock_plugin_state` during network I/O |
| `GET /api/marketplace/entries?q=&source=&category=` | entries with `installed`, `pluginId`, `updateAvailable`, `installable` |
| `POST /api/marketplace/install` `{ sourceId, entry }` | plugin for path entries, install job for git kinds |
| `POST /api/marketplace/plugins/:id/update` | or extend the existing plugin update to branch on `origin_source_id` |

Invalid URL, missing `marketplace.json`, and bad JSON return readable 4xx messages.

Displayed `marketplace.json` fields: `name`, `description`, `version`, `author`, `category`, `tags`, `homepage`.

## Placeholder environment fallback

`placeholders.rs` resolves `${VAR}` from plugin settings **or the server environment**. So `${GITHUB_TOKEN}` in the catalog `github` plugin would silently pick up a host `GITHUB_TOKEN`.

- Recommendation: state this in docs. The plugin card shows whether a value came from a setting or the environment.
- Open: add an opt-out so catalog plugins resolve from settings only. Decide before the catalog ships.

## Web

A **Marketplace** tab on the Plugins page (`web/src/features/plugins/`):

- search, plus source and category filters;
- entry cards with description, publisher badge, skill and MCP counts when known, and Install / Installed / Update. npm entries show "not installable" with the reason;
- source management: add, refresh, remove. Coppice cannot be removed.

After install, the user lands on the normal plugin card for settings and enable. Copy: "Coppice marketplace" and "available to all agents you give access to". No connector names.

## Unsupported

- npm sources
- Signing
- Plugin `commands/`, `agents/`, `hooks/`

## Testing

- **Unit:** catalog parse (every embedded entry → `expand_marketplace` resolves `Ok`); copy-install path traversal and outside-root symlinks; the cache-inside-plugin-dir check.
- **Integration (embedded DB, `allow_file_git_urls` with `file://` fixtures):** source CRUD and errors; entries and filters, including with git networking disabled for the built-in source; one-row install; 409 conflicts; git-subdir copies only the subfolder; update keeps row id, enabled state, settings, and access; source removal keeps plugins.
- **Cross-connector gateway test:** a catalog-installed plugin appears in `skill_list` and `tools/list` for two agents on different connector ids, and is absent for an agent without access. Assert nothing is written to the worktree or `HOME`.
- **Web:** tab, filters, copy, community badge, stdio confirmation.
- **Smoke:** extend `make e2e-smoke-m10` to install from the Coppice source.
- **Manual:** `github` on one live-verified connector (cursor or opencode) and one of claude-code or codex.
- **Final:** `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test`.

## Delivery order

1. This spec and the M10 Part 3 milestone entry (done in this step).
2. First-party catalog: `server/marketplace/coppice/` with the `github` plugin and its skills and `.mcp.json`.
3. Config and layout: `plugins.marketplaces_dir`, desktop wiring, backup exclusion, the startup check.
4. Backend: migration 054, `MarketplaceService`, copy-install, git-subdir job variant, update.
5. API routes.
6. Web tab.
7. Cross-connector tests, smoke, and docs (`docs/plugins.md`, `docs/architecture.md`, `website/`). Reword the "Claude Code marketplace" layout row as "the file format Coppice reads".
