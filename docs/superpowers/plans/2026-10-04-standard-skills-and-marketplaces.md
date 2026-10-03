# Standard Skills Repos and Plugin Marketplaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install `npx skills`-style repos and Claude Code marketplaces into Coppice unchanged, and let admins switch individual skills off.

**Architecture:** The folder scanner learns three layouts (marketplace → several plugin rows, plugin, skills package with `npx skills` discovery). Rows from one git clone share a `git_root`, so one Update refreshes them all. A per-plugin `disabled_skills` list filters the skill catalog.

**Tech Stack:** Rust (Axum, SQLx, embedded Postgres tests), `yaml-rust2` (new), React + TanStack Query + zod + Vitest.

**Spec:** `docs/superpowers/specs/2026-10-04-standard-skills-and-marketplaces-design.md`

## Global Constraints

- Every task's requirements include the spec; quoted error strings below are exact.
- Server owns state; handlers stay thin (logic in `server/src/services/` and `server/src/plugins/`).
- Mutations are admin-only and need `X-CSRF-Token` (existing `AdminUser` + CSRF middleware).
- Server tests: `cargo test -p coppice-server --features embedded-test-db <filter>`; never `make test` mid-task (final task only).
- `cargo clippy --workspace -- -D warnings` must pass after each task.
- Web: `make web-test`; do not run Prettier (the repo does not use it); keep eslint error count at the existing baseline for touched files.
- No secret values in responses or logs (unchanged behavior; do not log manifest settings).
- Container dirs (exact): `skills/`, `skills/.curated/`, `skills/.experimental/`, `skills/.system/`, `.agents/skills/`, `.claude/skills/`; walker depth 3; folder root depth 1.
- New status string: `external`. New columns: `plugins.git_root TEXT`, `plugins.disabled_skills TEXT[] NOT NULL DEFAULT '{}'`, `plugin_installs.plugin_ids UUID[] NOT NULL DEFAULT '{}'`.

## Review Focus

- A marketplace root that also has `plugin.json` and an entry `source: "./"` must yield exactly one row (the entry), not two — Task 3 test `marketplace_root_entry_yields_single_row`.
- Two marketplace entries resolving to the same folder must not fail the scan (first entry wins, second dropped with a warning) — Task 3 test `duplicate_entry_sources_keep_first`.
- A symlink inside a skills container pointing back at an ancestor must not loop or escape — Task 2 test `walker_ignores_symlink_loops_and_escapes`.
- A plugin installed from git before this change (backfilled `git_root`) must still update — Task 5 test `update_of_pre_migration_git_plugin_still_works`.
- Disabled skills survive Update/rescan; removed names are pruned; renamed skills reappear enabled — Task 6 test `disabled_skills_survive_rescan_and_prune_removed`.

---

### Task 1: YAML skill frontmatter

**Files:**
- Modify: `server/Cargo.toml` (add `yaml-rust2` via `cargo add yaml-rust2 -p coppice-server`)
- Modify: `server/src/plugins/skills.rs` (`parse_skill_file`, tests module)

**Interfaces:**
- Produces: `pub(crate) fn parse_skill_file(text: &str) -> anyhow::Result<(Frontmatter, String)>` — unchanged signature; `Frontmatter.description` is now whitespace-collapsed (single spaces, trimmed).

- [ ] **Step 1: Write failing unit tests in `skills.rs` tests**

```rust
#[test]
fn frontmatter_folded_description_is_one_line() {
    let (fm, body) = parse_skill_file("---\nname: pdf\ndescription: >\n  Fill PDF forms.\n  Use for PDFs.\n---\nBody\n").unwrap();
    assert_eq!(fm.name, "pdf");
    assert_eq!(fm.description, "Fill PDF forms. Use for PDFs.");
    assert_eq!(body, "Body\n");
}
#[test]
fn frontmatter_literal_description_collapses() {
    let (fm, _) = parse_skill_file("---\nname: a\ndescription: |\n  line one\n  line two\n---\n").unwrap();
    assert_eq!(fm.description, "line one line two");
}
#[test]
fn frontmatter_quoted_and_extra_keys() {
    let (fm, _) = parse_skill_file("---\nname: \"a-b\"\ndescription: 'Says: hi'\nlicense: MIT\nmetadata:\n  x: 1\n---\n").unwrap();
    assert_eq!((fm.name.as_str(), fm.description.as_str()), ("a-b", "Says: hi"));
}
#[test]
fn frontmatter_crlf_ok() {
    assert!(parse_skill_file("---\r\nname: a\r\ndescription: d\r\n---\r\nB").is_ok());
}
#[test]
fn frontmatter_errors() {
    let err = |t: &str| format!("{:#}", parse_skill_file(t).unwrap_err());
    assert!(err("---\ndescription: d\n---\n").contains("frontmatter `name` is required"));
    assert!(err("---\nname: a\n---\n").contains("frontmatter `description` is required"));
    assert!(err("---\nname: [1, 2]\ndescription: d\n---\n").contains("frontmatter `name` is required"));
    assert!(err("---\nname: a\ndescription: d\n  bad: [\n---\n").contains("invalid frontmatter YAML"));
    assert!(err("no frontmatter").contains("SKILL.md must start with YAML frontmatter"));
}
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --lib plugins::skills` — Expected: the folded/literal/YAML-error tests FAIL.

- [ ] **Step 3: Implement** — keep the existing `---` split and error wording; parse the YAML block with `yaml_rust2::YamlLoader::load_from_str`; error `invalid frontmatter YAML: <reason>`; take `name` / `description` only when they are YAML strings (non-string → the "is required" error); collapse description whitespace with `split_whitespace().collect::<Vec<_>>().join(" ")`.

- [ ] **Step 4: Run** `cargo test -p coppice-server --lib plugins::` — Expected: PASS (existing plugin tests unaffected).

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): parse SKILL.md frontmatter as YAML"`

---

### Task 2: Skill walker and skills-package discovery

**Files:**
- Create: `server/src/plugins/skill_walk.rs` (+ `pub mod skill_walk;` in `server/src/plugins/mod.rs`)
- Modify: `server/src/plugins/capability.rs` (`SkillsCapability::parse`, `skills_in`)
- Modify: `server/src/plugins/manifest.rs` (`is_plugin_root`)
- Create fixtures: `fixtures/plugins/skills-catalog/` (`skills/docs/pdf/SKILL.md` with a folded description, `skills/docs/pdf/deep/SKILL.md` (shadowed), `skills/web/design/SKILL.md`, `.agents/skills/agent-one/SKILL.md`, `node_modules/junk/SKILL.md`, `skills/a/b/c/d/SKILL.md` (depth 4, ignored)), `fixtures/plugins/single-skill/SKILL.md` (+ `scripts/run.sh`)
- Test: unit tests in `skill_walk.rs`, `capability.rs`, `manifest.rs`

**Interfaces:**
- Produces:
  - `pub const SKILL_CONTAINERS: [&str; 6] = ["skills", "skills/.curated", "skills/.experimental", "skills/.system", ".agents/skills", ".claude/skills"];`
  - `pub fn skill_dirs(root: &Path, container_rel: &str, max_depth: usize) -> Vec<String>` — rel paths (from `root`, `/`-separated, sorted) of dirs containing `SKILL.md`; `root` canonical; does not descend into a dir that has `SKILL.md`; skips names starting with `.` and `node_modules` below the container; skips entries whose canonical path is outside `root`; never follows a symlinked dir to a path already on the current walk path.
  - `pub fn has_skills_package(root: &Path) -> bool` — root `SKILL.md`, or any container (depth 3) or the root (depth 1) yields a skill.
  - Skills-package `SkillEntry` rules: root `SKILL.md` → single entry, `rel_path: ""`, `name` = folder name; else entries from all containers (depth 3) plus root (depth 1); `name` = last path segment; later duplicates by `rel_path` get `error: Some("duplicate skill name")`.

- [ ] **Step 1: Write failing tests**

```rust
// skill_walk.rs
#[test] fn walker_finds_flat_and_catalog_layouts() // skills-catalog: skill_dirs(root,"skills",3) == ["skills/docs/pdf","skills/web/design"]
#[test] fn walker_stops_at_depth_limit()            // "skills/a/b/c/d" absent
#[test] fn walker_skips_dot_dirs_and_node_modules() // root depth 1 never returns "node_modules/junk"
#[test] fn walker_ignores_symlink_loops_and_escapes() // tempdir: skills/x/SKILL.md, skills/loop -> .., skills/out -> /tmp; returns ["skills/x"], terminates
// capability.rs
#[test] fn skills_package_single_root_skill()      // single-skill: one entry, rel_path "", name "single-skill", error None
#[test] fn skills_package_catalog_and_agent_dirs()  // names == ["agent-one","design","pdf"], pdf description "Fill PDF forms. Use for PDFs."
#[test] fn duplicate_skill_names_marked()           // tempdir: skills/a/x/SKILL.md + .claude/skills/x/SKILL.md → second has error "duplicate skill name"
#[test] fn plugin_layout_skills_use_walker()        // tempdir with plugin.json + skills/cat/s/SKILL.md → one skill "s"
// manifest.rs
#[test] fn is_plugin_root_cases()                   // true: single-skill, skills-catalog, .claude/skills only; false: empty dir, dir with only README.md
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --lib plugins::` — Expected: new tests FAIL.

- [ ] **Step 3: Implement `skill_walk.rs`, switch `SkillsCapability`/`skills_in` to `skill_dirs`, and `is_plugin_root` to `plugin.json || marketplace.json || has_skills_package`** (`marketplace.json` path constant `".claude-plugin/marketplace.json"` lives in `capability.rs` as `pub(crate) const MARKETPLACE_JSON`). Keep the existing escape handling for `plugin.json` `skills` paths (`path escapes plugin root`).

- [ ] **Step 4: Run** `cargo test -p coppice-server --lib plugins::` and `cargo test -p coppice-server --features embedded-test-db --test integration_plugins` — Expected: PASS (existing fixtures `skills-only`, `superpowers-like`, `sample-plugin` unchanged).

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): npx-skills compatible skill discovery"`

---

### Task 3: Marketplace classification

**Files:**
- Create: `server/src/plugins/marketplace.rs` (+ `pub mod marketplace;`)
- Modify: `server/src/plugins/manifest.rs` (`PluginManifest` gains two fields)
- Modify: `server/src/plugins/discover.rs` (`discover`)
- Create fixture: `fixtures/plugins/marketplace-repo/` — `.claude-plugin/marketplace.json` with entries: `alpha` → `"./plugins/alpha"` (has `plugin.json` name `alpha`, one skill), `beta-skills` → `"./plugins/beta"` (skills package, no `plugin.json`), `remote-one` → `{ "source": "github", "repo": "acme/remote-one" }`, `escape` → `"../x"`, `ghost` → `"./plugins/ghost"` (missing)
- Test: unit tests in `marketplace.rs`, `discover.rs`

**Interfaces:**
- Consumes: Task 2 `is_plugin_root`, `parse_plugin`.
- Produces:
  - `PluginManifest { …, #[serde(default)] pub marketplace: Option<MarketplaceRef>, #[serde(default)] pub external: Option<ExternalSource> }`
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct MarketplaceRef { pub name: String }`
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct ExternalSource { pub kind: String, pub url: Option<String> }`
  - `pub fn expand_marketplace(folder: &Path, folder_rel: &str) -> Vec<Discovered>` — one `Discovered` per entry (rules in the spec, "Marketplace"); invalid file → one `Discovered { rel_path: folder_rel, name: <folder name>, result: Err("invalid marketplace.json: <reason>") }`.
  - Entry rel paths: in-repo → `folder_rel` joined with the normalized source (`"./"` → `folder_rel`); remote → `format!("{folder_rel}#{entry_name}")`. When `folder_rel` is `""` (plugin dir itself is the marketplace) use the source path alone / `#<name>`.
  - External rows: `Ok(PluginManifest { name, version: "0.0.0", description, layout: PluginLayout::SkillsOnly, skills: [], mcp_servers: [], unsupported: [], marketplace: Some(..), external: Some(..) , author: None })`.
  - URL derivation: `github` → `https://github.com/<repo>.git`; `git-subdir` → `url` (a bare `owner/repo` becomes `https://github.com/owner/repo.git`) and `kind` `"git-subdir:<path>"`; `url` → its `url`; other kinds → `url: None`, `kind` = the source's `source` string (or `"unknown"`).
  - Entry name validated with `is_valid_plugin_name` (invalid → `Err("marketplace entry \"<name>\": invalid plugin name")`).
  - `discover`: a folder with `marketplace.json` contributes only `expand_marketplace(...)` (never also itself as a plugin); dedupe by `rel_path`, first wins (warn the rest).

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn marketplace_fixture_rows() {
    let rows = expand_marketplace(&fixture("marketplace-repo"), "marketplace-repo");
    let by = |rel: &str| rows.iter().find(|d| d.rel_path == rel).unwrap();
    let alpha = by("marketplace-repo/plugins/alpha").result.as_ref().unwrap();
    assert_eq!(alpha.name, "alpha");
    assert_eq!(alpha.marketplace.as_ref().unwrap().name, "acme-market");
    assert_eq!(by("marketplace-repo/plugins/beta").name, "beta-skills");
    let remote = by("marketplace-repo#remote-one").result.as_ref().unwrap();
    assert_eq!(remote.external.as_ref().unwrap().url.as_deref(), Some("https://github.com/acme/remote-one.git"));
    assert_eq!(by("marketplace-repo#escape").result.as_ref().unwrap_err(), "marketplace entry \"escape\": source must stay inside the repository");
    assert_eq!(by("marketplace-repo#ghost").result.as_ref().unwrap_err(), "marketplace entry \"ghost\": source path does not exist");
}
#[test] fn invalid_marketplace_json_is_one_invalid_row()  // "{" → 1 row, Err starts with "invalid marketplace.json: "
#[test] fn marketplace_root_entry_yields_single_row()     // tempdir with plugin.json + marketplace.json entry "./" → discover(parent) has exactly one row for the folder, with marketplace set
#[test] fn duplicate_entry_sources_keep_first()           // two entries "./p" → one row, name of the first entry
#[test] fn git_subdir_and_npm_sources()                   // git-subdir {url:"acme/mono",path:"tools/x"} → url Some("https://github.com/acme/mono.git"), kind "git-subdir:tools/x"; npm → url None, kind "npm"
#[test] fn discover_expands_marketplace_child()           // plugin dir containing marketplace-repo → rows with rel paths prefixed "marketplace-repo/"
```

Error rows for in-repo entries use `rel_path` `folder_rel#<name>` (no folder to point at).

- [ ] **Step 2: Run** `cargo test -p coppice-server --lib plugins::` — Expected: FAIL.

- [ ] **Step 3: Implement** with `serde_json::Value` parsing (do not add a typed struct for unknown source kinds); normalize relative sources by stripping `./` and trailing `/`, rejecting absolute paths and any `..` component; target must be a directory **and** `is_plugin_root` (else the "source path does not exist" error for missing dirs, and `parse_plugin`'s error for non-plugins).

- [ ] **Step 4: Run** `cargo test -p coppice-server --lib plugins::` — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): expand Claude Code marketplaces at scan time"`

---

### Task 4: Persist new layouts (migration, statuses, API fields)

**Files:**
- Create: `server/migrations/033_plugin_marketplaces_and_skill_switches.sql`
- Modify: `server/src/services/plugin_service.rs` (`PLUGIN_COLUMNS`, `PluginRow`, `row_to_plugin`, `upsert_discovered`, `set_enabled`, `start_update`)
- Modify: `server/src/api/plugins.rs` (`PluginResponse`, `test_plugin`)
- Modify: `server/tests/common/mod.rs` only if `truncate_test_workspace` needs new tables (none expected)
- Test: `server/tests/integration_plugins.rs`

**Interfaces:**
- Consumes: Task 3 `PluginManifest.marketplace` / `.external`.
- Produces:
  - Migration: widen `plugins.status` check to `('ok','invalid','missing','shadowed','external')`; `ALTER TABLE plugins ADD COLUMN git_root TEXT, ADD COLUMN disabled_skills TEXT[] NOT NULL DEFAULT '{}'`; `UPDATE plugins SET git_root = rel_path WHERE source = 'git'`; `ALTER TABLE plugin_installs ADD COLUMN plugin_ids UUID[] NOT NULL DEFAULT '{}'`; `UPDATE plugin_installs SET plugin_ids = ARRAY[plugin_id] WHERE plugin_id IS NOT NULL`.
  - `PluginRow { …, pub git_root: Option<String>, pub disabled_skills: Vec<String> }`.
  - `upsert_discovered`: `Ok(m)` with `m.external.is_some()` → status `external`; shadowing query unchanged (it only ranks `ok`).
  - `set_enabled(id, true)` on `external` → `PluginError::Conflict("external plugins cannot be enabled")`.
  - `start_update` on `external` → `PluginError::Conflict("external plugins are installed from their own repository")` (checked before the git check).
  - `POST /api/plugins/{id}/test` on `external` → 409 (existing not-`ok` path; keep message).
  - `PluginResponse` gains `marketplace: Option<MarketplaceRef>`, `external: Option<ExternalSource>`, `git_root: Option<String>` (camelCase `gitRoot`); `skills[]` gains `enabled: bool` (true unless in `disabled_skills`) — use a response struct `SkillResponse { name, description, rel_path, error, enabled }`.

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test] async fn marketplace_dir_lists_rows_and_external()  // add dir with marketplace-repo → alpha ok (marketplace.name "acme-market"), beta-skills ok, remote-one status "external" + external.url, escape/ghost invalid
#[tokio::test] async fn skills_catalog_and_single_skill_dirs()      // add dir with both fixtures → skills-catalog skills names ["agent-one","design","pdf"], all enabled true; single-skill 1 skill
#[tokio::test] async fn external_cannot_be_enabled_tested_or_updated() // PATCH enabled → 409 "external plugins cannot be enabled"; POST test → 409; POST update → 409 "external plugins are installed from their own repository"
#[tokio::test] async fn external_not_assignable_to_agents()         // PUT /api/agents/{id}/plugins with external id → existing rejection status
#[tokio::test] async fn git_install_sets_git_root()                 // install_sample → gitRoot == relPath; local plugins have gitRoot null
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins -- marketplace_dir external_ skills_catalog git_install_sets_git_root` — Expected: FAIL.

- [ ] **Step 3: Implement** the migration and service/API changes above.

- [ ] **Step 4: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins` — Expected: PASS (whole file, including `plugins_api_shape_unchanged` updated for the new keys).

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): persist marketplace and external plugin rows"`

---

### Task 5: Git installs and updates of whole clones

**Files:**
- Modify: `server/src/services/plugin_service.rs` (`PluginInstall`, `INSTALL_COLUMNS`, `row_to_install`, `start_update`, `finish_install`, `abandon_install` if it matches on `plugin_id`)
- Modify: `server/src/api/plugins.rs` (`PluginInstallResponse.plugin_ids`, `update`, `spawn_git_job`)
- Create fixtures via test helper: a git repo built from `fixtures/plugins/marketplace-repo` and `skills-catalog` (reuse the existing `git()` helper and `file://` installs in `integration_plugins.rs`)
- Test: `server/tests/integration_plugins.rs`

**Interfaces:**
- Consumes: Task 4 `PluginRow.git_root`.
- Produces:
  - `PluginInstall { …, pub plugin_ids: Vec<Uuid> }`; API `pluginIds`.
  - Clone rows: `plugin_dir_id = dir AND (rel_path = $root OR rel_path LIKE $root || '/%' OR rel_path LIKE $root || '#%') AND status <> 'missing'`. `finish_install` stamps `source='git'`, git url/ref/commit, `git_root = dest_rel` on all of them (disable only on fresh install), sets `plugin_ids` (ordered by `rel_path`) and `plugin_id` = first. Zero rows → existing failure path with the new message `no plugin, skills, or marketplace found in this repository` (both install and update).
  - `start_update(plugin_id) -> Result<(PluginInstall, PathBuf, String), PluginError>` — returns the clone path `<dir>/<git_root or rel_path>` and `git_root`; "already running" conflict checks any running install whose `plugin_ids` overlaps the clone's row ids.
  - `spawn_git_job(state, install_id, dest_rel, updated: Vec<Uuid>, job)` — after a successful update, `stop_plugin` for every id in the finished install's `plugin_ids` (re-read after completion so new siblings are included).
  - Update tooltip data: none server-side (web counts siblings by `gitRoot`).

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test] async fn install_marketplace_repo_creates_sibling_rows()   // install from file:// repo of marketplace-repo → install.pluginIds.len() == 5 (alpha, beta-skills, remote-one, escape, ghost: every row from the clone, any status), all rows gitRoot == "marketplace-repo", source "git", same gitCommit, all disabled
#[tokio::test] async fn install_skills_catalog_repo()                      // pluginIds.len()==1, skills 3
#[tokio::test] async fn install_of_repo_without_skills_fails()            // README-only repo → failed, error "no plugin, skills, or marketplace found in this repository", clone removed
#[tokio::test] async fn update_refreshes_all_siblings_and_stops_servers() // commit adds plugins/gamma entry + changes alpha skill; POST update on beta → all siblings new gitCommit, gamma appears disabled, alpha MCP server (use inline fake-mcp in alpha) health "stopped"
#[tokio::test] async fn update_removed_entry_becomes_missing()            // remove beta entry upstream → beta status "missing", agent assignment kept
#[tokio::test] async fn update_of_pre_migration_git_plugin_still_works()  // install_sample, then SQL `UPDATE plugins SET git_root = NULL`; push a commit; update → succeeds (start_update falls back to rel_path) and git_root is set afterwards
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins -- install_ update_` — Expected: new tests FAIL.

- [ ] **Step 3: Implement** the service/API changes above.

- [ ] **Step 4: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins` — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): one git install or update covers every plugin in the clone"`

---

### Task 6: Skill switches

**Files:**
- Modify: `server/src/services/plugin_service.rs` (new `set_skill_enabled`; prune in `rescan`; pass disabled list in `refresh_catalog`)
- Modify: `server/src/plugins/skills.rs` (`PluginSkillSet::from_manifest` signature)
- Modify: `server/src/api/plugins.rs` (route + handler)
- Test: `server/tests/integration_plugins.rs`, unit test in `skills.rs`

**Interfaces:**
- Consumes: Task 4 `PluginRow.disabled_skills`, `SkillResponse.enabled`.
- Produces:
  - `pub fn from_manifest(plugin_id: Uuid, root: &Path, manifest: &PluginManifest, disabled: &[String]) -> PluginSkillSet` — excludes skills whose `name` is in `disabled`.
  - `pub async fn set_skill_enabled(&self, plugin_id: Uuid, skill: &str, enabled: bool) -> Result<PluginRow, PluginError>` — `NotFound` for unknown plugin or a skill name not in the manifest; adds/removes the name (no duplicates).
  - `rescan`: after upserting an `ok` row, `disabled_skills` keeps only names present in its manifest.
  - Route `PUT /api/plugins/{plugin_id}/skills/{skill}` body `{ "enabled": bool }` → `PluginResponse`; handler calls `set_skill_enabled` then `refresh_catalog(&state.skills)`.

- [ ] **Step 1: Write failing tests**

```rust
// skills.rs unit
#[test] fn from_manifest_skips_disabled() // manifest skills a,b; disabled ["b"] → ids ["p:a"]
// integration
#[tokio::test] async fn skill_switch_hides_skill_from_agents()       // enable superpowers-like, assign to agent, PUT skills/brainstorming {enabled:false} → response skill enabled false; mint run token (common::mint_test_token_with_plugins) → MCP skill_list lacks "superpowers-like:brainstorming", skill_load returns error containing "not found"; `state.skills.skills_for(&[plugin_id])` lacks it; re-enable → listed again
#[tokio::test] async fn skill_switch_unknown_is_404()                // unknown skill → 404; unknown plugin → 404
#[tokio::test] async fn skill_switch_requires_admin_and_csrf()       // non-admin 403; missing CSRF → existing CSRF status
#[tokio::test] async fn disabled_skills_survive_rescan_and_prune_removed() // disable writing-plans; rescan → still disabled; delete its folder + rescan → disabledSkills pruned (re-adding folder → enabled true)
```

- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins -- skill_switch disabled_skills` and `cargo test -p coppice-server --lib plugins::skills` — Expected: FAIL.

- [ ] **Step 3: Implement** the above.

- [ ] **Step 4: Run** the same commands plus `cargo clippy --workspace -- -D warnings` — Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(plugins): switch individual plugin skills off"`

---

### Task 7: Web — schema, plugin card, install result, agent picker

**Files:**
- Modify: `web/src/lib/schemas/plugin.ts` (status `external`; `skills[].enabled` default `true`; `marketplace`, `external`, `gitRoot` nullable/default null; install `pluginIds` default `[]`)
- Modify: `web/src/features/plugins/usePlugins.ts` (`useSetSkillEnabled`)
- Modify: `web/src/features/plugins/PluginCard.tsx`, `PluginsPage.tsx` (install prefill + success list), `web/src/features/agents/AgentForm.tsx` (count enabled skills)
- Test: `PluginCard.test.tsx`, `PluginsPage.test.tsx`, `AgentForm.test.tsx`

**Interfaces:**
- Consumes: API fields from Tasks 4–6; `PUT /api/plugins/{id}/skills/{name}`.
- Produces:
  - `useSetSkillEnabled(pluginId: string)` → mutation `({ skill, enabled }) => Plugin`; updates the plugins query cache.
  - `PluginCard` prop `onInstallFromGit?: (url: string) => void`; `PluginsPage` passes a handler that sets the Install form's Git URL and scrolls the form into view (`scrollIntoView?.()`).
  - `PluginCard` prop `siblingCount?: number` (count of plugins with the same non-null `gitRoot`, computed in `PluginsPage`).
- Exact copy: `Skills (12 of 30 on)` / `Skills (3)` when all on; switch `aria-label` `Disable <skill>` / `Enable <skill>`; `From marketplace <name>`; external: `Lives in another repository: <url>` or `Source: <kind>`; button `Install from git`; Update `title` `Updates all N plugins from this repository` when `siblingCount > 1`; install success `Installed <repo folder>: N plugins (a, b, c)` when `pluginIds.length > 1` (names from the refreshed plugin list), else existing `Installed <url>.`; status hint for `external`: `Lives in another repository; install it separately.`

- [ ] **Step 1: Write failing tests**

```tsx
it('skill switches show the on count and PUT the change')        // skills a(enabled), b(disabled) → header "Skills (1 of 2 on)"; click switch "Enable b" → PUT /api/plugins/<id>/skills/b body {"enabled":true}
it('external plugin card shows the source and offers install')   // status external, external {kind:'github', url:'https://github.com/acme/r.git'} → text, no switch role, button click calls onInstallFromGit(url)
it('marketplace plugins show where they came from')              // "From marketplace acme-market"
it('update tooltip names sibling count')                         // siblingCount 3 → Update title "Updates all 3 plugins from this repository"
it('Install from git on an external card fills the install form') // PluginsPage: click → Git URL input value equals url
it('install success lists every plugin from the clone')          // poll returns succeeded with pluginIds [a,b] → "Installed marketplace-repo: 2 plugins (alpha, beta-skills)"
it('agent picker counts enabled skills only')                    // AgentForm: plugin with 3 skills, 1 disabled → "(2 skills)"
```

- [ ] **Step 2: Run** `cd web && npx vitest run src/features/plugins src/features/agents` — Expected: FAIL.

- [ ] **Step 3: Implement** the above.

- [ ] **Step 4: Run** `make web-test`, `cd web && npx eslint src/features/plugins src/features/agents && npx tsc -b --noEmit` — Expected: tests PASS; eslint errors not above baseline (6 in `features/agents`, 0 in `features/plugins`); tsc clean.

- [ ] **Step 5: Commit** — `git commit -m "feat(web): skill switches, marketplace and external plugin cards"`

---

### Task 8: Docs and final verification

**Files:**
- Modify: `docs/plugins.md` (supported layouts incl. `npx skills` and marketplaces, skill switches, `external` status in the status table, Update covers the whole repo, troubleshooting rows for marketplace errors)
- Modify: `docs/architecture.md` (plugin discovery: three layouts, walker rules, `git_root`, `disabled_skills`)
- Modify: `docs/superpowers/specs/2026-10-04-standard-skills-and-marketplaces-design.md` (status line + tick met acceptance boxes; leave manual acceptance unticked)

- [ ] **Step 1: Update the docs** — every factual claim must match the code (error strings, container list, depth, statuses).
- [ ] **Step 2: Run** `make test` — Expected: all pass (rerun once if only `knowledge_query_plan_has_relational_indexes` flakes).
- [ ] **Step 3: Run** `cargo clippy --workspace -- -D warnings` and `make web-test` — Expected: clean / pass.
- [ ] **Step 4: Run** `make e2e-smoke-m10` (default Docker stack only) — Expected: passes.
- [ ] **Step 5: Commit** — `git commit -m "docs: standard skills repos and marketplaces"`
