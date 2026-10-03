# Standard Skills Repos and Plugin Marketplaces — Design

**Status:** implemented (2026-10-04) on branch `standard-skills`; manual acceptance (real repositories) pending. See [Implementation notes](#implementation-notes) for deviations from the design. Extends [M10 plugins](2026-09-29-m10-plugins-design.md) (Plugin format and discovery, Plugin lifecycle). User guide: [docs/plugins.md](../../plugins.md).

## Goal

Install skills the way the rest of the ecosystem publishes them. A repository that works with `npx skills add` (vercel-labs/skills) or a Claude Code plugin marketplace (`.claude-plugin/marketplace.json`) should install into Coppice with **Install from git** or by adding its folder as a plugin directory, without restructuring it. Admins can switch individual skills off so a large catalog does not flood agents' skill lists.

## Decisions (2026-10-04)

| Topic | Decision |
|-------|----------|
| Approach | Expand at scan time: one folder (or clone) can yield several plugin rows. No new marketplace entity. |
| Skill selection | Whole plugin is attached to agents (unchanged); admins switch individual skills off per plugin, workspace-wide. |
| Remote marketplace entries | Not fetched. Listed as `external` rows with their URL and an "Install from git" shortcut. |
| Skill discovery rules | Follow `npx skills`: root `SKILL.md`, known container dirs, up to 3 levels, shallower shadows deeper. |
| Frontmatter | Real YAML parsing (maintained crate); fixes multi-line descriptions. |

## Current state (baseline)

- `discover(dir)` treats `dir` as one plugin if `is_plugin_root(dir)` (has `.claude-plugin/plugin.json`, a `skills/` dir, or a child with `SKILL.md`), else each such direct child.
- Skills-only folders read `skills/*/SKILL.md` or `*/SKILL.md` — one level only. A root `SKILL.md` is not recognized; git install of a single-skill repo fails ("not a plugin").
- `parse_skill_file` reads frontmatter line by line; `description: >` yields the literal `>`.
- `marketplace.json` is ignored.
- A git install records one `plugin_installs.plugin_id`; Update pulls the plugin's own folder.

## Folder classification

Each folder Coppice examines (a plugin directory itself, or each direct child, as today) is classified in this order.

### 1. Marketplace

The folder has `.claude-plugin/marketplace.json`. Parse `plugins` (array). Each entry yields one plugin row:

- **In-repo source** — `source` is a string path relative to the marketplace root (`"./plugins/foo"`, `"plugins/foo"`, or `"./"`).
  - The path must not be absolute or contain `..`; otherwise the row is `invalid` with error `marketplace entry "<name>": source must stay inside the repository`.
  - The target must be a directory; otherwise `invalid` with `marketplace entry "<name>": source path does not exist`.
  - The target is parsed as a **plugin** (rule 2) or **skills package** (rule 3). The row's `rel_path` is the marketplace folder's rel path joined with the normalized source (e.g. `anthropic-skills/plugins/foo`; `"./"` → the marketplace folder itself).
  - Name: the target's `plugin.json` `name` if present, else the entry `name`.
- **Remote source** — `source` is an object (`github`, `git-subdir`, `url`, `npm`, `archive`, `command`, or unknown). The row has status **`external`**, `rel_path` `<marketplace rel>#<entry name>`, name = entry name, description = entry `description`. Its manifest records `external: { kind, url }` where `url` is: `github` → `https://github.com/<repo>.git`; `git-subdir` → its `url` (expanding `owner/repo` shorthand to GitHub) plus `path`; `url` → its `url`; others → no URL.
- Every row from a marketplace records `marketplace: { name }` in its manifest (shown on the card).
- Invalid `marketplace.json` (bad JSON, `plugins` not an array) → one `invalid` row for the folder with the parse error.
- Marketplace entries are not themselves treated as marketplaces (no nesting).

### 2. Plugin

The folder has `.claude-plugin/plugin.json`. Unchanged, except skill directories (default `skills/`, or the `skills` paths in `plugin.json`) are searched with the skill walker below.

### 3. Skills package

No `plugin.json`. Name = folder name, version `0.0.0` (unchanged).

- If the folder has a root `SKILL.md`, the folder is a single skill (skill `rel_path` `""`), and nothing else is searched.
- Otherwise search these containers with the skill walker: `skills/`, `skills/.curated/`, `skills/.experimental/`, `skills/.system/`, `.agents/skills/`, `.claude/skills/`. The folder root itself is searched **one level** only (`<name>/SKILL.md`, today's behavior).
- A folder is a skills package only if at least one `SKILL.md` is found; otherwise it is not a plugin (skipped by discovery, and a git install of it fails).

### Skill walker

From a container directory, walk up to **3 levels** deep (`c/<a>/SKILL.md`, `c/<a>/<b>/SKILL.md`, `c/<a>/<b>/<c>/SKILL.md`):

- A directory containing `SKILL.md` is a skill; its subdirectories are not searched (shallower shadows deeper).
- Skip directories whose name starts with `.` and `node_modules` (container paths listed above are entered explicitly).
- Do not follow symlinks that resolve outside the plugin root (existing escape check applies).
- Duplicate skill names within one plugin: first by `rel_path` wins; others are listed with error `duplicate skill name`.

### Detection

`is_plugin_root(folder)` becomes: has `marketplace.json`, or `plugin.json`, or the skills-package rule finds at least one `SKILL.md`. Discovery depth is unchanged (the directory itself, else direct children).

## Skill frontmatter

- Parse the block between the leading `---` lines as YAML with a maintained crate (e.g. `yaml-rust2` / `saphyr`; chosen in the plan).
- `name` and `description` are required non-empty strings; anything else is an error for that skill (plugin stays usable). Other keys (`license`, `allowed-tools`, `metadata`, …) are ignored.
- The stored description is whitespace-collapsed to one line (folded and literal blocks both become one line for the skill index).
- Errors keep the current wording where it exists (`frontmatter \`name\` is required`, …) plus `invalid frontmatter YAML: <reason>`.

## Git install and update

- Clone as today into `<plugin dir>/<repo name>`. After the scan, the install succeeds if the clone folder produced **at least one row** (any status). Otherwise the clone is removed and the install fails with `no plugin, skills, or marketplace found in this repository`.
- New column `plugins.git_root TEXT` — the rel path of the clone folder. Every row produced from the clone has `source = git`, the clone's `git_url` / `git_ref` / `git_commit`, and the same `git_root`. Backfill: existing git rows get `git_root = rel_path`.
- New column `plugin_installs.plugin_ids UUID[] NOT NULL DEFAULT '{}'` — all rows the clone produced. `plugin_id` stays (first id) for compatibility. The install API returns `pluginIds`.
- **Update** on any row with a `git_root` pulls `<dir>/<git_root>` once, rescans, and records the new commit on every row with that `git_root`. All sibling plugins' MCP servers are stopped (as Update does today for one plugin). Rows whose entry disappeared become `missing` (assignments kept); new entries appear disabled. Update of an `external` row is rejected (409, `external plugins are installed from their own repository`).
- A local folder that is a marketplace (added as a plugin directory) expands the same way with `source = local`; Rescan picks up changes.
- Shadowing is unchanged: names are unique workspace-wide; earlier plugin directory wins, then lower `rel_path`.

## External rows

- Status `external` (migration widens the `plugins.status` check).
- Cannot be enabled (PATCH → 409 `external plugins cannot be enabled`), assigned to agents (existing "only enabled ok" rule), tested, or updated. They have no skills, MCP servers, or settings.
- They are rescanned like any row and become `missing` if the entry is removed.

## Skill switches

- New column `plugins.disabled_skills TEXT[] NOT NULL DEFAULT '{}'` (skill names). Keyed by name, so it survives rescans and updates; a renamed skill reappears enabled. Names no longer present are dropped on the next successful scan of that plugin.
- `PUT /api/plugins/{id}/skills/{name}` `{ "enabled": bool }` — admin, CSRF. Unknown plugin or skill name → 404. Returns the plugin.
- Plugin responses: each `skills[]` item gains `enabled: bool`.
- `SkillCatalog` excludes disabled skills from `skills_for` (context skill index, `skill_list`) and `get` (`skill_load` → `skill "<id>" not found`). Takes effect immediately, including in running runs.
- The bundled `coppice` plugin's skills cannot be switched off (not in the plugin table).

## UI

- **Plugin card**
  - Skills header: `Skills (12 of 30 on)` (or `Skills (3)` when all on); each skill row has a switch (`aria-label` "Disable <skill>" / "Enable <skill>").
  - Marketplace rows show `From marketplace <name>` under the description.
  - Git rows with siblings: Update tooltip `Updates all N plugins from this repository`.
  - `external` rows: status pill `external` (hint: "Lives in another repository; install it separately."), text `Lives in another repository: <url>` (or `Source: <kind>` with no URL), and an **Install from git** button (only with a URL) that fills the Install form's Git URL and scrolls to it. No enable switch, Test, settings, or skills.
- **Install success** lists produced plugins: `Installed <repo>: 3 plugins (a, b, c)`; single plugin keeps `Installed <url>.`
- **Agent form** plugin picker counts enabled skills only.
- Status hint for `external` added to the card's status hints; the guide panel is unchanged.

## API summary

| Change | Detail |
|--------|--------|
| `GET /api/plugins`, `GET /api/plugins/{id}` | `status` may be `external`; `skills[].enabled`; `marketplace: { name } \| null`; `external: { kind, url } \| null`; `gitRoot` |
| `PUT /api/plugins/{id}/skills/{name}` | new; toggles a skill |
| `PATCH /api/plugins/{id}` | enabling an `external` row → 409 |
| `POST /api/plugins/{id}/update` | updates the whole clone; `external` → 409 |
| `GET /api/plugin-installs/{id}` | gains `pluginIds` |

## Error handling

| Case | Result |
|------|--------|
| Bad `marketplace.json` | One `invalid` row for the folder, error `invalid marketplace.json: <reason>` |
| Entry source with `..` / absolute | `invalid` row for that entry |
| Entry source path missing | `invalid` row for that entry |
| Skill YAML error | Skill listed with error; plugin `ok` |
| Clone yields nothing | Install fails, clone removed |
| Toggle unknown skill | 404 |
| Enable / update / test `external` | 409 |

## Testing

- **Unit (server):** YAML frontmatter (folded `>`, literal `|`, quoted, missing / non-string `name` or `description`, extra keys ignored, CRLF); skill walker (root `SKILL.md`, each container, depth 1–3, depth 4 ignored, shadowing, dot-dirs and `node_modules` skipped, duplicate names); marketplace classification (relative entries, `"./"`, `..`, absolute, missing path, remote kinds → `external` with derived URL, bad JSON); `is_plugin_root` cases.
- **Integration (embedded Postgres):** new fixtures `fixtures/plugins/marketplace-repo` (two in-repo plugins, one skills-only entry, one `github` entry), `skills-catalog` (`skills/<category>/<name>/SKILL.md`, a multi-line description), `single-skill` (root `SKILL.md`). Cases: add dir with each fixture → expected rows/statuses; git install of `marketplace-repo` from a local `file://` repo → rows share `git_root`, install `pluginIds`; update pulls once and moves every sibling to the new commit and stops their servers; removed entry → `missing`; toggle skill off → absent from `skill_list`, `skill_load` 404-style error, and run context skill index; toggle unknown → 404; enable / update `external` → 409; external not assignable; migration backfills `git_root`.
- **Web:** skill switches + header count; external card text and Install-from-git prefill; install success list; agent picker skill count.
- **Manual acceptance:** Install from git `https://github.com/anthropics/skills` and `https://github.com/vercel-labs/agent-skills`; skills listed with correct descriptions; switch one off; attach to a mock agent and confirm the skill index in a run.

## Out of scope

- Fetching remote marketplace entries automatically.
- Per-agent skill selection.
- Installing a subfolder of a repository (`/tree/main/skills/x` URLs).
- Bundled skill files (`scripts/`, `references/`) inside a sandbox — M11.
- Plugin `commands/`, `agents/`, `hooks/` (still unsupported).

## Implementation notes

Amendments decided during implementation; the code and [docs/plugins.md](../../plugins.md) follow these where they differ from the sections above.

- **Detection at depth 0.** A registered plugin directory itself (`is_plugin_dir_root`) is one plugin only with `plugin.json`, `marketplace.json`, a root `SKILL.md`, or a container dir (`skills/`, `.agents/skills/`, …) yielding a skill. The one-level `*/SKILL.md` root search does not apply there; children of the directory use the full rule (`is_plugin_root`, including the one-level search). Otherwise a single-skill git clone inside a plugin directory would collapse the whole directory into one plugin.
- **Symlink escapes.** Skills reached through a symlink resolving outside the plugin root are listed with error `path escapes plugin root` (not silently dropped).
- **Marketplace errors.** Besides the two errors above: `marketplace entry "<name>": no plugin or skills found at source` (target has neither `plugin.json` nor a skills package), `marketplace entry "<name>": source must be a path or an object` (missing / other type), and `marketplace entry "<name>": invalid plugin name` (empty, invalid, or reserved entry name). Error rows and external rows use rel_path `<marketplace rel>#<entry>`; only successful in-repo rows use the joined source path. Entries resolving to the same rel_path: the first wins.
- **`git-subdir`.** The URL is the entry's `url` (shorthand expanded); the path is recorded in the kind (`git-subdir:<path>`), not appended to the URL.
- **Row name.** The target's `plugin.json` name if present, else the entry name; skills-only targets validate the entry name, not the folder name.
- **Skill names.** A skill's name is its folder name (a root `SKILL.md` takes the plugin folder's name). Duplicate names: only error-free skills claim a name, so an erroring skill never shadows a valid one.
- **Installs.** `plugin_installs.plugin_id` records the first produced row by rel_path; `plugin_ids` lists all.

## Acceptance criteria

- [x] Repos in `npx skills` layouts (root `SKILL.md`, `skills/` flat or catalog, `.agents/skills`, `.claude/skills`) install via git and via plugin directory.
- [x] Marketplace repos expand into one plugin per in-repo entry; remote entries listed as `external` with an install shortcut.
- [x] Multi-line YAML descriptions parse correctly.
- [x] One Update refreshes every plugin from the same clone.
- [x] Admins can switch individual skills off; disabled skills are invisible to agents immediately.
- [ ] `docs/plugins.md` and `docs/architecture.md` updated; `make test`, clippy, `make web-test` pass; `make e2e-smoke-m10` passes. (Docs, tests, clippy, web tests done; smoke not yet run — ports held by a local dev stack.)
