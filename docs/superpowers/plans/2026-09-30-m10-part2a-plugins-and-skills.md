# M10 Part 2a — Plugins and Plugin Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make OpenCode tool-first with a per-run `opencode serve`, and let admins register plugin directories, install plugins from git, enable them, and assign them to agents so runs can list and load plugin skills.

**Architecture:** OpenCode moves from one global `opencode serve` to a per-run process owned by a lease in a server-wide registry; every consumer that used the global base URL looks it up by run id. Plugins are parsed from disk by pure functions in `server/src/plugins/`, persisted and resolved (shadowing, missing, enablement) by `services/plugin_service.rs`, and published to the in-memory `SkillCatalog`, which serves built-in skills plus skills of the plugins snapshotted on the run token.

**Tech Stack:** Rust (Axum 0.8, SQLx 0.8, Tokio), Postgres, `git` on PATH, React + TanStack Query. No new crates (`serde_json`, `tempfile`, `reqwest` already in `server/Cargo.toml`).

**Spec:** [docs/superpowers/specs/2026-09-29-m10-plugins-design.md](../specs/2026-09-29-m10-plugins-design.md) — this plan implements "Part 2a" in Delivery order: the OpenCode per-run section and step 5. Plugin MCP proxy, plugin settings, the Test button, and the Tools & Skills tab are Part 2b.

## Global Constraints

- Server owns state: handlers in `server/src/api/` stay thin; rules live in `server/src/services/plugin_service.rs` and pure helpers in `server/src/plugins/`.
- Admin-only for all plugin and plugin-dir mutations (`AdminUser` extractor); CSRF applies to all `/api` mutations. `GET /api/plugins*` and `/api/agents/:id/plugins` are available to any authenticated user; `PUT /api/agents/:id/plugins` follows agent update (`AuthUser`) and accepts only `enabled` plugins with status `ok`.
- Installed plugins start `enabled = false`. The bundled `coppice` plugin is not a row in `plugins`, is always on, and `coppice` is a reserved plugin name.
- Plugin skill identity: `<plugin>:<skill>`; built-ins are served as `<skill>` and also resolve as `coppice:<skill>`.
- Scan depth: the dir itself (rel_path `""`) and each direct child. If the dir itself is a plugin, its children are not scanned.
- Plugin names match `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`; anything else is `invalid`.
- Git install is the only server-side `git clone`; repositories keep the no-clone rule. Git runs with `GIT_TERMINAL_PROMPT=0`, a `[plugins] git_timeout_secs` timeout (default 300), and `--` before URL/path arguments.
- Allowed git URLs: `https://…`, `ssh://…`, `git@host:path`. `file://` only when `[plugins] allow_file_git_urls = true` (default `false`; tests set it). URLs and refs starting with `-`, and `ext::` URLs, are rejected.
- Token/secret rules from Part 1 still hold: `COPPICE_MCP_TOKEN` reaches OpenCode only via process env; the per-run config file references `{env:COPPICE_MCP_TOKEN}` and is written under `<artifacts_dir>/runs/<run_id>/`, never the worktree or a repo checkout.
- `plugin_ids` on a run token is the snapshot of the agent's assigned plugins that are `enabled` and `ok` at mint time; the catalog additionally serves only plugins that are still `enabled` and `ok`.
- Fast verification while iterating: targeted `cargo test -p coppice-server --features embedded-test-db <filter>`, `make test-unit`, `make web-test`. `make test` + `cargo clippy --workspace -- -D warnings` + `make web-test` only at the end, then `make clean`.

## Review Focus

1. **Paths escaping the plugin root** — a `plugin.json` `skills` override like `../../etc` or a skill folder symlinked outside the plugin must mark that skill invalid, never serve it. (Task 3: `skills_override_outside_root_is_invalid`, `symlinked_skill_outside_root_is_invalid`.)
2. **Git argument injection** — `--upload-pack=…`, `-oProxyCommand`, `ext::sh -c …`, and refs like `--output=x` must be rejected before `git` runs. (Task 6: `validate_git_url_rejects_injection`, `validate_ref_rejects_option_like`.)
3. **Dir reorder flips the shadowing winner** — after reorder + rescan the old winner becomes `shadowed` and is not served even though `enabled` stays true; enabling a `shadowed` plugin is refused. (Task 4: `reorder_then_rescan_flips_winner_and_unserves_loser`.)
4. **Plugin disappears and comes back** — a deleted plugin folder becomes `missing` (skills stop being served), agent assignments survive, and the plugin serves again after it returns and a rescan runs. (Task 5: `missing_plugin_keeps_assignment_and_returns`.)
5. **OpenCode process leaks** — cancel, stop, provider error, and panic paths must kill the per-run `opencode serve`; graceful server shutdown kills all of them. (Task 2: `lease_drop_kills_process_and_forgets_base_url`, `shutdown_all_kills_every_server`.)

---

## File Structure

```text
server/src/sessions/opencode_run_server.rs   # NEW: per-run opencode serve registry + lease + run config JSON
server/src/sessions/opencode_serve.rs        # DELETE (global serve)
server/src/providers/opencode.rs             # per-run server wiring
server/tests/support/fake_opencode.rs        # NEW: fake `opencode serve` test binary
server/src/plugins/manifest.rs               # NEW: parse one plugin root (plugin / skills-only)
server/src/plugins/discover.rs               # NEW: scan a plugin dir (depth 0/1)
server/src/plugins/git_install.rs            # NEW: validate git url/ref, clone/update, repo name
server/src/plugins/skills.rs                 # SkillCatalog gains plugin skills
server/src/services/plugin_service.rs        # NEW: dirs, rescan, shadowing, enable, assignment, installs
server/src/api/plugins.rs                    # NEW: /api/plugin-dirs, /api/plugins, /api/plugin-installs, /api/agents/:id/plugins
server/migrations/029_plugins.sql            # NEW
config/src/lib.rs                            # PluginsConfig
fixtures/plugins/                            # NEW: sample plugins
web/src/lib/schemas/plugin.ts                # NEW
web/src/features/plugins/{usePlugins.ts,PluginsPage.tsx,PluginsPage.test.tsx}  # NEW
web/src/features/agents/AgentForm.tsx        # Plugins picker
```

---

### Task 1: Verify OpenCode per-run `opencode serve` against a live CLI

A verification task, not product code. Output is a spec update.

**Files:**
- Modify: `docs/superpowers/specs/2026-09-29-m10-plugins-design.md` (OpenCode wiring row + "OpenCode: per-run `opencode serve`" section)

**Interfaces:**
- Produces: the verified `opencode.json` shape (config key names, header interpolation syntax) and answers to checks a–e below, which Task 2's `opencode_run_config` must match.

- [ ] **Step 1: Get a working OpenCode CLI with model credentials.** On the dev host (where the Part 1 Cursor probe ran), install OpenCode (`coppice connector install opencode` targets the managed Docker `$HOME`; on the host use OpenCode's official installer). Confirm `opencode --version` and that a plain `opencode run "say hi"` answers. If no model credentials are available, stop and ask the user to log in — do not guess results.

- [ ] **Step 2: Start the probe.** `PORT=5099 cargo run -p coppice-server --example mcp_probe` (token `probe-token`).

- [ ] **Step 3: Run the checks** with a scratch workspace and a run dir outside it, e.g. `/tmp/oc-run/opencode.json`:

```json
{"$schema":"https://opencode.ai/config.json","mcp":{"coppice":{"type":"remote","url":"http://127.0.0.1:5099/mcp","enabled":true,"headers":{"Authorization":"Bearer {env:COPPICE_MCP_TOKEN}"}}}}
```

Start `env OPENCODE_CONFIG=/tmp/oc-run/opencode.json COPPICE_MCP_TOKEN=probe-token opencode serve --hostname 127.0.0.1 --port 4101`, then drive it (`opencode run --attach http://127.0.0.1:4101 "Call the coppice ping tool with message hello and print the result."`, or the HTTP session API Coppice uses: `POST /session`, `POST /session/:id/prompt_async`). Record:
  - a. Does `pong hello` come back and the probe log `authorized=true`? Exact config keys that worked; tool-name form in the event stream.
  - b. Does the global config / auth still apply (same model providers as without `OPENCODE_CONFIG`)?
  - c. Two `opencode serve` processes on different ports at the same time, each with its own config — do both answer?
  - d. Session created on process A, A killed, prompt the same session id on process B in the same directory — does it resume?
  - e. Any permission prompt for MCP tools in serve mode? Nothing written into the workspace?

- [ ] **Step 4: Update the spec.** Replace "Part 2a: unverified — decided mechanism" with "verified (OpenCode `<version>`)" and write the results under the OpenCode section like the Cursor one. If a or c fails, stop and report to the user before Task 2. If d fails, record that chat turns on OpenCode start a fresh session each turn (Task 2 then clears `resume_session_id` for OpenCode).

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-09-29-m10-plugins-design.md
git commit -m "docs(m10): record OpenCode per-run serve verification"
```

---

### Task 2: OpenCode per-run `opencode serve`

**Files:**
- Create: `server/src/sessions/opencode_run_server.rs`, `server/tests/support/fake_opencode.rs`, `server/tests/integration_opencode_run_server.rs`
- Delete: `server/src/sessions/opencode_serve.rs`
- Modify: `server/src/sessions/mod.rs`, `server/src/providers/opencode.rs`, `server/src/providers/registry.rs`, `server/src/lib.rs` (`AppState.opencode_serve` → `opencode_runs`), `server/src/main.rs` (startup, `sweep_orphaned_runs`, shutdown), `server/src/api/ws/live.rs`, `server/src/workers/run_watchdog.rs`, `server/src/workers/health_worker.rs`, `server/src/services/agent_health.rs`, `server/tests/common/mod.rs`, `server/tests/integration_opencode_live.rs`, `server/Cargo.toml`, `config/src/lib.rs` (doc: `serve_port` ignored), `config.example.toml`, `deploy/config/config.example.toml`, `docs/providers/README.md`

**Interfaces:**
- Consumes: `McpAccess { url, token }` and `McpAccess::env()` (`server/src/mcp/grant.rs`); `providers::run_dir(input, "opencode") -> Result<PathBuf, ProviderError>`; `OpenCodeClient::with_run_timeout(base_url, timeout)`; Task 1's verified config shape.
- Produces:
  - `pub fn opencode_run_config(access: Option<&McpAccess>) -> serde_json::Value`
  - `pub struct OpenCodeRunServers` with `pub fn new(command: String, hostname: String) -> Arc<Self>`, `pub async fn start(self: &Arc<Self>, key: &str, config_path: &Path, env: Vec<(&'static str, String)>) -> anyhow::Result<OpenCodeRunLease>`, `pub fn base_url(&self, key: &str) -> Option<String>`, `pub async fn shutdown_all(&self)`
  - `pub struct OpenCodeRunLease` with `pub fn base_url(&self) -> &str`, `pub async fn stop(self)`, and `Drop` that kills the child and removes the key
  - `AppState.opencode_runs: Arc<OpenCodeRunServers>` (always present); `ConnectorRegistry::from_config(config, opencode_runs: Arc<OpenCodeRunServers>)`

- [ ] **Step 1: Write failing unit tests** in `opencode_run_server.rs`:

```rust
#[test]
fn run_config_registers_coppice_remote_server_without_plaintext_token() {
    let access = McpAccess { url: "http://127.0.0.1:5000/mcp".into(), token: "secret-run-token".into() };
    let cfg = opencode_run_config(Some(&access));
    assert_eq!(cfg["mcp"]["coppice"]["url"], "http://127.0.0.1:5000/mcp");
    assert_eq!(cfg["mcp"]["coppice"]["headers"]["Authorization"], "Bearer {env:COPPICE_MCP_TOKEN}");
    assert!(!cfg.to_string().contains("secret-run-token"));
}

#[test]
fn run_config_without_access_has_no_mcp() {
    assert!(opencode_run_config(None).get("mcp").is_none());
}
```

and in `providers/opencode.rs` replace `opencode_mcp_setup_refuses_tool_first_runs` with `opencode_config_path_is_under_run_dir_not_worktree` (config path == `run_dir(input,"opencode")/opencode.json`, not under the worktree from `context_path`), plus `opencode_tool_first_run_without_run_dir_is_mcp_unavailable` (`mcp: Some`, `artifacts_dir: None` → error string starts with `invalid input: mcp_unavailable:`).

- [ ] **Step 2: Add the fake binary** `server/tests/support/fake_opencode.rs`, registered in `server/Cargo.toml` as `[[bin]] name = "fake-opencode"`, `path = "tests/support/fake_opencode.rs"`, `required-features = ["embedded-test-db"]`. It accepts `serve --hostname H --port P`, writes `{"opencodeConfig": $OPENCODE_CONFIG, "hasToken": bool, "pid": u32}` to `$FAKE_OPENCODE_RECORD` if set, and serves `GET /doc` → 200 until killed.

- [ ] **Step 3: Write failing integration tests** in `server/tests/integration_opencode_run_server.rs` using `env!("CARGO_BIN_EXE_fake-opencode")` as the command:
  - `run_server_spawns_with_per_run_config_and_token_env` — record file shows `opencodeConfig` == the passed path and `hasToken == true`; `servers.base_url("run-1") == Some(lease.base_url())`.
  - `two_leases_get_distinct_ports` — two concurrent leases have different base URLs and both `/doc` answer 200.
  - `lease_drop_kills_process_and_forgets_base_url` — after `drop(lease)`, `base_url("run-1")` is `None` and within 5 s `/doc` stops answering.
  - `shutdown_all_kills_every_server` — two leases alive, `shutdown_all().await`, both `/doc` stop answering.
  - `start_with_missing_binary_names_the_command` — command `does-not-exist-opencode` → error message contains `does-not-exist-opencode` and `not found on PATH`.

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db opencode_run`
Expected: FAIL (module/functions not defined).

- [ ] **Step 5: Implement `opencode_run_server.rs`.** Free port: bind `std::net::TcpListener` on `<hostname>:0`, read the port, drop it, spawn `<command> serve --hostname <hostname> --port <port>` with `.kill_on_drop(true)`, env `OPENCODE_CONFIG=<config_path>` plus `env`. Retry up to 3 times if the child exits before healthy (port race). Move `wait_for_healthy` / `probe_health` (`GET /doc`) from `opencode_serve.rs` here. The registry holds `Mutex<HashMap<String, RunServer { base_url, child }>>`; the lease removes its entry and `start_kill()`s on `stop`/`Drop`. Keep the "not found on PATH" message from the old manager. `opencode_run_config` builds Task 1's verified shape.

- [ ] **Step 6: Wire the provider.** `OpenCodeProvider::new(servers: Arc<OpenCodeRunServers>, config)`. In `run`: if `input.mcp` is `Some`, config path = `run_dir(&input, "opencode")?.join("opencode.json")` (map missing dir to `mcp_unavailable("opencode has no run artifacts dir for its per-run config")`); if `None` (drafts/probes), write the config into a `tempfile::TempDir` held until the run ends. Key = `input.run_id` or a fresh UUID. Start the lease with `access.env()`, run `OpenCodeClient::with_run_timeout(lease.base_url(), …).run_session(…)` exactly as today, then `lease.stop().await` on every exit path (Drop covers `?`/panic). Delete `opencode_mcp_setup`.

- [ ] **Step 7: Move consumers to per-run lookup.**
  - `AppState`: replace `opencode_serve` with `opencode_runs: Arc<OpenCodeRunServers>`, built in `main.rs` from `config.agent.connectors.opencode.{command, serve_hostname}`; `test_state` builds one with command `opencode`.
  - `ConnectorRegistry::from_config`: register `opencode` when `connectors.opencode.enabled || default_connector == "opencode"`.
  - `main.rs`: stop starting a global serve; `sweep_orphaned_runs` interrupts every active OpenCode run that has a session (no per-run server survives a restart); graceful shutdown calls `state.opencode_runs.shutdown_all()`.
  - `api/ws/live.rs` recovery: `state.opencode_runs.base_url(&run_id.to_string())`; `None` → `mark_recovery_interrupted(state, run_id, "server restarted during run")`.
  - `run_watchdog.rs`: same lookup; `None` → mark interrupted with reason `"opencode server lost during run"` and call `handle_terminal_run` as the existing `Ok(None)` branch does.
  - `agent_health.rs` / `health_worker.rs`: drop the serve parameter; OpenCode reports `Healthy` after the model-provider check, like the other CLI connectors.
  - `config/src/lib.rs`: doc comment on `serve_port`: "Ignored since M10 Part 2a (each run gets a free port); kept so existing config files still parse." Update both example configs' comments.

- [ ] **Step 8: Run tests**

Run: `cargo test -p coppice-server --features embedded-test-db opencode` then `cargo test -p coppice-server --features embedded-test-db --test integration_live_console`
Expected: PASS. Fix `integration_opencode_live.rs` / `common/mod.rs` construction sites for the new `AppState` field.

- [ ] **Step 9: Update `docs/providers/README.md`** OpenCode section: per-run `opencode serve`, `OPENCODE_CONFIG` under the run dir, `serve_port` ignored.

- [ ] **Step 10: Commit**

```bash
git add -A server config config.example.toml deploy/config/config.example.toml docs/providers/README.md
git commit -m "feat(m10): per-run opencode serve makes OpenCode tool-first"
```

---

### Task 3: Plugin manifest parsing and discovery

Pure filesystem code, no DB.

**Files:**
- Create: `server/src/plugins/manifest.rs`, `server/src/plugins/discover.rs`, `fixtures/plugins/` (below)
- Modify: `server/src/plugins/mod.rs`, `server/src/plugins/skills.rs` (make `parse_skill_file` `pub(crate)`)

Fixtures:

```text
fixtures/plugins/sample-plugin/.claude-plugin/plugin.json   # {"name":"sample-plugin","version":"1.2.0","description":"Sample","author":{"name":"Coppice"}}
fixtures/plugins/sample-plugin/skills/hello/SKILL.md        # name: hello, description: Says hello
fixtures/plugins/sample-plugin/skills/broken/SKILL.md       # no frontmatter
fixtures/plugins/sample-plugin/commands/do-thing.md
fixtures/plugins/sample-plugin/hooks/hooks.json
fixtures/plugins/sample-plugin/.mcp.json                    # {"mcpServers":{"echo":{"command":"echo-mcp"},"remote":{"type":"http","url":"https://example.com/mcp"},"old":{"type":"sse","url":"https://example.com/sse"}}}
fixtures/plugins/skills-only/review/SKILL.md                # name: review, description: Reviews code
fixtures/plugins/superpowers-like/.claude-plugin/plugin.json + skills/brainstorming/SKILL.md + skills/writing-plans/SKILL.md + agents/code-reviewer.md
```

**Interfaces:**
- Produces:

```rust
// plugins/manifest.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub name: String,
    pub version: String,              // "0.0.0" for skills-only / missing
    pub description: String,          // "" when absent
    pub author: Option<String>,
    pub layout: PluginLayout,         // Plugin | SkillsOnly  (serde: "plugin" | "skillsOnly")
    pub skills: Vec<SkillEntry>,
    pub mcp_servers: Vec<McpServerEntry>,
    pub unsupported: Vec<String>,     // subset of ["commands", "agents", "hooks"], sorted
}
pub struct SkillEntry { pub name: String, pub description: String, pub rel_path: String, pub error: Option<String> }
pub struct McpServerEntry { pub name: String, pub kind: McpServerKind }  // Stdio | Http | Sse | Unknown
pub fn is_plugin_root(root: &Path) -> bool;
pub fn parse_plugin(root: &Path) -> Result<PluginManifest, String>;   // Err = plugin-level invalid reason
pub fn is_valid_plugin_name(name: &str) -> bool;

// plugins/discover.rs
pub struct Discovered { pub rel_path: String, pub name: String, pub result: Result<PluginManifest, String> }
pub fn discover(dir: &Path) -> std::io::Result<Vec<Discovered>>;    // sorted by rel_path
```

`is_plugin_root`: has `.claude-plugin/plugin.json`, or a `skills/` dir, or a direct child containing `SKILL.md`. `Discovered.name` is the manifest name, or the folder name (dir basename for rel_path `""`) when parsing failed.

- [ ] **Step 1: Write failing tests** in `manifest.rs` / `discover.rs` against `fixtures/plugins` (path via `env!("CARGO_MANIFEST_DIR")/../fixtures/plugins`):
  - `parses_claude_plugin_layout` — sample-plugin: name `sample-plugin`, version `1.2.0`, author `Some("Coppice")`, layout Plugin, skills `hello` (error None, rel_path `skills/hello`) and `broken` (error Some), `unsupported == ["commands","hooks"]`, mcp servers `echo`=Stdio, `remote`=Http, `old`=Sse.
  - `parses_skills_only_folder` — skills-only: name `skills-only`, version `0.0.0`, layout SkillsOnly, one skill `review`.
  - `parses_superpowers_shaped_plugin` — two valid skills, `unsupported == ["agents"]`.
  - `invalid_plugin_json_is_plugin_level_error` — temp dir with `plugin.json` = `{` → `Err` containing `plugin.json`.
  - `reserved_or_bad_name_is_invalid` — names `coppice` and `bad name` → `Err`.
  - `skills_override_outside_root_is_invalid` — `plugin.json` `"skills": "../../etc"` → no skills loaded from outside; manifest has a `SkillEntry` with `error == Some("path escapes plugin root")` or `Err`, never a skill outside root.
  - `symlinked_skill_outside_root_is_invalid` (`#[cfg(unix)]`) — `skills/evil` symlink to a dir outside the plugin → entry error `path escapes plugin root`.
  - `discover_depth0_plugin_skips_children` — dir that is itself a plugin → exactly one entry with rel_path `""`.
  - `discover_depth1_children` — `fixtures/plugins` → three entries `sample-plugin`, `skills-only`, `superpowers-like`.
  - `discover_ignores_deeper_nesting_and_files` — `a/b/.claude-plugin/plugin.json` and a loose file → no entries.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p coppice-server --lib plugins::`
Expected: FAIL (modules not defined).

- [ ] **Step 3: Implement.** `plugin.json` fields: `name` (required), `version`, `description`, `author` (string or `{name}`), optional `skills` (string or array of relative paths; default `["skills"]`). Every skill path is canonicalized and must `starts_with` the canonical plugin root, else `error: "path escapes plugin root"`. Skills-only: skills are `<root>/skills/*/SKILL.md` if `skills/` exists, else `<root>/*/SKILL.md`. Skill `name` = directory name; frontmatter via `parse_skill_file`. `.mcp.json`: read `mcpServers` object keys; kind Stdio if `command` present, Http if `type == "http"` or (`url` and no `type`), Sse if `type == "sse"`, else Unknown.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server --lib plugins::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add server/src/plugins fixtures/plugins
git commit -m "feat(m10): parse Claude Code/Cursor plugins and skills-only folders"
```

---

### Task 4: Plugin storage, plugin dirs, rescan, enablement

**Files:**
- Create: `server/migrations/029_plugins.sql`, `server/src/services/plugin_service.rs`, `server/src/api/plugins.rs`, `server/tests/integration_plugins.rs`
- Modify: `server/src/services/mod.rs`, `server/src/api/mod.rs`, `config/src/lib.rs` (`PluginsConfig`), `server/src/main.rs` (startup), `deploy/docker-compose.yml`, `config.example.toml`, `deploy/config/config.example.toml`

Migration `029_plugins.sql`:

```sql
CREATE TABLE plugin_dirs (
    id UUID PRIMARY KEY, path TEXT NOT NULL UNIQUE, position INTEGER NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE plugins (
    id UUID PRIMARY KEY,
    plugin_dir_id UUID NOT NULL REFERENCES plugin_dirs(id) ON DELETE CASCADE,
    rel_path TEXT NOT NULL, name TEXT NOT NULL, version TEXT NOT NULL DEFAULT '0.0.0',
    description TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'local' CHECK (source IN ('local', 'git')),
    git_url TEXT, git_ref TEXT, git_commit TEXT,
    manifest JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('ok', 'invalid', 'missing', 'shadowed')),
    error TEXT, enabled BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (plugin_dir_id, rel_path)
);
CREATE TABLE agent_plugins (
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, plugin_id)
);
CREATE TABLE plugin_installs (
    id UUID PRIMARY KEY,
    plugin_dir_id UUID NOT NULL REFERENCES plugin_dirs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('install', 'update')),
    git_url TEXT NOT NULL, git_ref TEXT,
    plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    error TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), finished_at TIMESTAMPTZ
);
ALTER TABLE run_tool_tokens ADD COLUMN plugin_ids UUID[] NOT NULL DEFAULT '{}';
ALTER TABLE agent_presets ADD COLUMN default_plugins TEXT[] NOT NULL DEFAULT '{}';
```

**Interfaces:**
- Consumes: `plugins::discover::discover`, `PluginManifest` (Task 3).
- Produces:

```rust
// config/src/lib.rs — AppConfig.plugins: PluginsConfig (serde default)
pub struct PluginsConfig { pub dir: String /* "./data/plugins" */, pub allow_file_git_urls: bool /* false */, pub git_timeout_secs: u64 /* 300 */ }

// services/plugin_service.rs
pub struct PluginService<'a> { pool: &'a PgPool }
pub enum PluginError { NotFound, Validation(String), Conflict(String), Db(sqlx::Error), Io(std::io::Error) }  // thiserror
pub struct PluginDir { pub id: Uuid, pub path: String, pub position: i32, pub is_default: bool }
pub struct PluginRow { pub id: Uuid, pub plugin_dir_id: Uuid, pub rel_path: String, pub name: String, pub version: String,
    pub description: String, pub source: String, pub git_url: Option<String>, pub git_ref: Option<String>,
    pub git_commit: Option<String>, pub manifest: Option<PluginManifest>, pub status: String,
    pub error: Option<String>, pub enabled: bool }
impl PluginService<'_> {
    pub fn new(pool: &PgPool) -> PluginService<'_>;
    pub async fn ensure_default_dir(&self, path: &str) -> Result<PluginDir, PluginError>;   // creates dir on disk + row (position 0, is_default)
    pub async fn list_dirs(&self) -> Result<Vec<PluginDir>, PluginError>;                    // by position
    pub async fn add_dir(&self, path: &str) -> Result<PluginDir, PluginError>;               // absolute, existing dir, canonicalized, unique; appended
    pub async fn move_dir(&self, id: Uuid, position: i32) -> Result<Vec<PluginDir>, PluginError>; // renumbers 0..n
    pub async fn remove_dir(&self, id: Uuid) -> Result<(), PluginError>;                     // default dir → Conflict
    pub async fn rescan(&self) -> Result<Vec<PluginRow>, PluginError>;
    pub async fn list_plugins(&self) -> Result<Vec<PluginRow>, PluginError>;
    pub async fn get_plugin(&self, id: Uuid) -> Result<PluginRow, PluginError>;
    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<PluginRow, PluginError>; // enabling non-ok → Conflict
    pub async fn plugin_path(&self, id: Uuid) -> Result<PathBuf, PluginError>;              // <dir>/<rel_path>
}
```

API (`api/plugins.rs`), camelCase JSON; `PluginResponse` = `PluginRow` fields + `skills`, `mcpServers`, `unsupported` flattened from the manifest:

```text
GET    /api/plugin-dirs                → [PluginDir]
POST   /api/plugin-dirs {path}         → 201 PluginDir, then rescan         (admin)
PATCH  /api/plugin-dirs/{id} {position}→ [PluginDir]                        (admin)
DELETE /api/plugin-dirs/{id}           → 204; default dir → 409             (admin)
POST   /api/plugins/rescan             → [PluginResponse]                   (admin)
GET    /api/plugins                    → [PluginResponse]
GET    /api/plugins/{id}               → PluginResponse
PATCH  /api/plugins/{id} {enabled}     → PluginResponse; non-ok → 409       (admin)
```

Error mapping: `NotFound`→404, `Validation`→400, `Conflict`→409, others→500 with `{ "error": msg }` like `api/repos.rs`.

- [ ] **Step 1: Write failing integration tests** in `server/tests/integration_plugins.rs` (use `bootstrap_and_login_with_state`, `state.db`, temp dirs; copy `fixtures/plugins/*` into them):
  - `default_dir_exists_after_bootstrap_and_cannot_be_removed` — `GET /api/plugin-dirs` has one `isDefault: true`; `DELETE` → 409.
  - `add_dir_scans_plugins_disabled_by_default` — add a temp dir holding the three fixtures → `GET /api/plugins` has three rows, all `enabled: false`, sample-plugin `status: "ok"`, `unsupported: ["commands","hooks"]`, skills include `hello` valid and `broken` with an error.
  - `add_dir_rejects_relative_missing_and_duplicate` — `"relative/x"` → 400; `/nonexistent-…` → 400; same path twice → 409.
  - `invalid_manifest_is_listed_with_error` — plugin with `plugin.json` `{` → `status: "invalid"`, `error` contains `plugin.json`, `enabled` cannot be set (409).
  - `rescan_marks_deleted_plugin_missing` — delete `skills-only` folder, `POST /api/plugins/rescan` → that row `status: "missing"`.
  - `same_name_in_two_dirs_second_is_shadowed` — dirs A then B both contain `sample-plugin` → A's row `ok`, B's row `shadowed`; `PATCH` enable B → 409.
  - `reorder_then_rescan_flips_winner_and_unserves_loser` — enable A's row; move B to position 0 via PATCH (triggers rescan) → B `ok`, A `shadowed` with `enabled: true`.
  - `plugin_mutations_require_admin_and_csrf` — member user → 403 on POST dir / PATCH plugin / rescan; missing CSRF → 403.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_plugins`
Expected: FAIL (routes 404 / table missing).

- [ ] **Step 3: Implement migration, config, service, API.** `rescan` runs in one transaction guarded by `SELECT pg_advisory_xact_lock(hashtext('coppice_plugin_rescan'))`: for each dir by position call `discover`; upsert by `(plugin_dir_id, rel_path)` (keep `enabled`, `git_*`; set name/version/description/manifest/error/status `ok` or `invalid`); rows not found → `missing`; then among `ok` rows group by `name` and mark all but the lowest dir position `shadowed`. A dir that no longer exists on disk marks its plugins `missing` (no error). `move_dir` and `add_dir`/`remove_dir` call `rescan` afterwards. `main.rs`: after migrate, `ensure_default_dir(&config.plugins.dir)` then `rescan()`; log and continue on scan errors. Test bootstrap in `tests/common/mod.rs` does the same with a per-test temp default dir.

- [ ] **Step 4: Docker + config.** `deploy/docker-compose.yml` server env `COPPICE_PLUGINS__DIR: /data/plugins`, volume `plugin_data:/data/plugins` (add to top-level `volumes`). Add a commented `[plugins]` section to both example configs.

- [ ] **Step 5: Run tests**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_plugins`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add server/migrations/029_plugins.sql server/src config deploy config.example.toml server/tests
git commit -m "feat(m10): plugin dirs, scan, shadowing, enablement API"
```

---

### Task 5: Plugin skills in runs — agent assignment, presets, token snapshot, catalog

**Files:**
- Modify: `server/src/plugins/skills.rs`, `server/src/services/plugin_service.rs`, `server/src/api/plugins.rs`, `server/src/mcp/token.rs`, `server/src/mcp/tools/skills.rs`, `server/src/workers/job_worker.rs` (mint sites at the `NewRunToolScope { … }` literals and `skills_for` calls), `server/src/workers/job_worker/compaction.rs`, `server/src/services/agent_service.rs` (`create_from_preset`), `server/src/main.rs`, `server/tests/integration_plugins.rs`, `server/tests/integration_mcp.rs`
- Create: `fixtures/agent-responses/mcp/plugin_skill_tool_call.json`

**Interfaces:**
- Consumes: `PluginService`, `PluginRow`, `PluginManifest.skills` (Tasks 3–4).
- Produces:

```rust
// plugins/skills.rs — SkillCatalog keeps `builtin` and adds interior-mutable plugin skills
pub struct PluginSkillSet { pub plugin_id: Uuid, pub plugin_name: String, pub root: PathBuf, pub skills: Vec<SkillInfo> } // SkillInfo.id = "<plugin>:<skill>"
impl SkillCatalog {
    pub fn set_plugin_skills(&self, sets: Vec<PluginSkillSet>);                 // replaces all (RwLock<HashMap<Uuid, PluginSkillSet>>)
    pub fn skills_for(&self, plugin_ids: &[Uuid]) -> Vec<SkillInfo>;            // builtins first (sorted), then plugin skills sorted by id
    pub fn get(&self, plugin_ids: &[Uuid], id: &str) -> Option<(SkillInfo, String)>; // plugin body read from disk at call time
}
// services/plugin_service.rs
pub async fn refresh_catalog(&self, catalog: &SkillCatalog) -> Result<(), PluginError>;   // enabled && ok rows, valid skills only
pub async fn agent_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError>;   // all assigned (for the picker)
pub async fn set_agent_plugins(&self, agent_id: Uuid, plugin_ids: &[Uuid]) -> Result<Vec<Uuid>, PluginError>; // each must be enabled && ok else Validation
pub async fn run_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError>;     // assigned ∩ enabled ∩ ok — the token snapshot
pub async fn apply_preset_defaults(&self, agent_id: Uuid, names: &[String]) -> Result<(), PluginError>; // silently skips names not enabled/ok
// mcp/token.rs
NewRunToolScope.plugin_ids: Vec<Uuid>; RunToolScope.plugin_ids: Vec<Uuid>   // stored in run_tool_tokens.plugin_ids
```

API: `GET /api/agents/{id}/plugins` → `{ "pluginIds": [...] }`; `PUT /api/agents/{id}/plugins { pluginIds }` → same shape; unknown agent → 404, non-enabled/non-ok plugin → 400.

`refresh_catalog` is called after every `rescan`, `set_enabled`, and (Task 6) install/update completion — the API handlers call it with `&state.skills`; startup calls it after the initial rescan.

- [ ] **Step 1: Write failing unit tests** in `skills.rs`:
  - `plugin_skills_are_namespaced_and_scoped_to_snapshot` — set one plugin set (sample fixture, id P); `skills_for(&[P])` contains `sample-plugin:hello`; `skills_for(&[])` has only the 6 built-ins.
  - `builtin_resolves_with_and_without_coppice_prefix` — `get(&[], "coppice-git")` and `get(&[], "coppice:coppice-git")` both `Some`.
  - `plugin_skill_body_reflects_disk_changes` — rewrite the fixture copy's `SKILL.md` body after `set_plugin_skills`; `get` returns the new body without frontmatter.
  - `plugin_skill_not_in_snapshot_is_none` — `get(&[], "sample-plugin:hello")` → `None`.

- [ ] **Step 2: Write failing integration tests** in `integration_plugins.rs`:
  - `set_agent_plugins_accepts_only_enabled_ok_plugins` — disabled plugin → 400; after enabling → 200 and GET returns it.
  - `missing_plugin_keeps_assignment_and_returns` — enable + assign; delete folder + rescan → `run_plugin_ids` empty but `GET /api/agents/{id}/plugins` still lists it; restore folder + rescan → `run_plugin_ids` has it again.
  - `preset_default_plugins_applied_when_enabled` — set `agent_presets.default_plugins = '{sample-plugin}'` for one preset via SQL, enable sample-plugin, create agent from that preset → agent's plugins contain it; with the plugin disabled → empty, creation still 201.
  And in `integration_mcp.rs`:
  - `mock_run_loads_plugin_skill_over_mcp` — `bootstrap_and_login_with_gateway("mcp/plugin_skill_tool_call.json")`, add fixture dir, enable sample-plugin, assign to the agent, run a ticket → run succeeds; `run_tool_calls` has `tool = 'skill_load'`, `status = 'ok'`; the run token row's `plugin_ids` contains the plugin id.
  - `plugin_skill_not_assigned_is_not_found` — same fixture without assignment → the `skill_load` call row has `status = 'error'` and the run still completes via `result_submit`.

Fixture `plugin_skill_tool_call.json`: `toolCalls: [{"tool":"skill_list","args":{}},{"tool":"skill_load","args":{"name":"sample-plugin:hello"}},{"tool":"result_submit","args":{…same done result as ticket_submit_result.json…}}]` plus the same final result.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db plugin_skill` and `--test integration_plugins`
Expected: FAIL.

- [ ] **Step 4: Implement.** In `job_worker.rs` compute `let plugin_ids = PluginService::new(pool).run_plugin_ids(run.agent_id).await.unwrap_or_default();` once per run before building context and minting; pass to `skills_for(&plugin_ids)` and `NewRunToolScope { plugin_ids, .. }` (compaction too). Skill tools use `&ctx.scope.plugin_ids`. `create_from_preset` calls `apply_preset_defaults(agent.id, &preset.default_plugins)` after insert (add `default_plugins` to `AgentPreset`).

- [ ] **Step 5: Run tests**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_plugins --test integration_mcp` and `cargo test -p coppice-server --lib plugins::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add server fixtures/agent-responses/mcp/plugin_skill_tool_call.json
git commit -m "feat(m10): serve assigned plugin skills to runs via token snapshot"
```

---

### Task 6: Git install and update

**Files:**
- Create: `server/src/plugins/git_install.rs`
- Modify: `server/src/plugins/mod.rs`, `server/src/services/plugin_service.rs`, `server/src/api/plugins.rs`, `server/src/main.rs` (fail stale `running` installs at startup), `server/tests/integration_plugins.rs`

**Interfaces:**
- Consumes: `PluginsConfig { allow_file_git_urls, git_timeout_secs }`, `PluginService::rescan`, `refresh_catalog`.
- Produces:

```rust
// plugins/git_install.rs (pure + process helpers)
pub fn validate_git_url(url: &str, allow_file: bool) -> Result<(), String>;
pub fn validate_ref(git_ref: &str) -> Result<(), String>;          // non-empty, no leading '-', no whitespace/control, no ".."
pub fn repo_dir_name(url: &str) -> Result<String, String>;        // last path segment, strip ".git", [A-Za-z0-9._-] only, not "."/".."
pub async fn clone(url: &str, git_ref: Option<&str>, dest: &Path, timeout: Duration) -> Result<String /*commit*/, String>;
pub async fn update(root: &Path, git_ref: Option<&str>, timeout: Duration) -> Result<String /*commit*/, String>;
// services/plugin_service.rs
pub struct PluginInstall { pub id: Uuid, pub plugin_dir_id: Uuid, pub kind: String, pub git_url: String, pub git_ref: Option<String>,
    pub plugin_id: Option<Uuid>, pub status: String, pub error: Option<String> }
pub async fn start_install(&self, git_url: &str, git_ref: Option<&str>, plugin_dir_id: Uuid, cfg: &PluginsConfig) -> Result<(PluginInstall, PathBuf /*dest*/), PluginError>;
pub async fn start_update(&self, plugin_id: Uuid) -> Result<(PluginInstall, PathBuf /*root*/), PluginError>; // non-git plugin → Validation
pub async fn finish_install(&self, id: Uuid, result: Result<String, String>, dest_rel: &str, catalog: &SkillCatalog) -> Result<PluginInstall, PluginError>;
pub async fn get_install(&self, id: Uuid) -> Result<PluginInstall, PluginError>;
pub async fn fail_stale_installs(&self) -> Result<u64, PluginError>;  // status running → failed, error "server restarted"
```

`clone`: `git clone --depth 1 [--branch <ref>] -- <url> <dest>`, then `git -C <dest> rev-parse HEAD`. `update`: `git -C <root> fetch --depth 1 origin <ref or HEAD>` then `git -C <root> checkout --detach FETCH_HEAD`, then `rev-parse`. Both use `tokio::process::Command` with env `GIT_TERMINAL_PROMPT=0`, `tokio::time::timeout`, and return git's stderr (trimmed) as the error. A failed clone removes a partially created `dest`.

API:

```text
POST /api/plugins/install {gitUrl, ref?, pluginDirId} → 202 PluginInstall  (admin); invalid url/ref → 400; dest exists → 409
POST /api/plugins/{id}/update                         → 202 PluginInstall  (admin); non-git plugin → 400
GET  /api/plugin-installs/{id}                        → PluginInstall
```

The handler validates synchronously, inserts the `running` row, then `tokio::spawn`s `clone`/`update` followed by `finish_install`, which on success runs `rescan`, sets `source = 'git'`, `git_url`, `git_ref`, `git_commit` on the row at `(plugin_dir_id, dest_rel)`, links `plugin_id`, and calls `refresh_catalog`; on failure stores the git error.

- [ ] **Step 1: Write failing unit tests** in `git_install.rs`:
  - `validate_git_url_accepts_https_ssh_scp` — `https://github.com/a/b.git`, `ssh://git@host/a/b`, `git@github.com:a/b.git` → Ok.
  - `validate_git_url_rejects_injection` — `--upload-pack=touch x`, `-oProxyCommand=x`, `ext::sh -c x`, `file:///tmp/x` (allow_file false), `` → Err; `file:///tmp/x` with allow_file true → Ok.
  - `validate_ref_rejects_option_like` — `--output=x`, `-b`, `a b`, `a..b`, `` → Err; `main`, `v1.2.0` → Ok.
  - `repo_dir_name_strips_git_suffix` — `https://github.com/obra/superpowers.git` → `superpowers`; `git@h:a/b` → `b`; `https://h/` → Err.

- [ ] **Step 2: Write failing integration tests** in `integration_plugins.rs` (config `plugins.allow_file_git_urls = true`; build a local bare repo from `fixtures/plugins/sample-plugin` with `git init` / commit / `git clone --bare` in a temp dir; poll `GET /api/plugin-installs/{id}` up to 20 s):
  - `install_from_git_clones_scans_and_records_commit` — install → `succeeded`; plugin row `source: "git"`, `gitCommit` == bare repo HEAD, `enabled: false`.
  - `install_into_existing_dest_is_conflict` — second install of the same URL into the same dir → 409.
  - `install_failure_is_reported` — `file:///nonexistent/repo.git` → install `failed` with non-empty `error`, no plugin row, no leftover folder.
  - `update_pulls_new_commit` — commit a change to the bare repo's source and push; `POST /api/plugins/{id}/update` → `succeeded`; `gitCommit` changed; new skill visible after install finishes.
  - `stale_running_install_failed_on_startup` — insert a `running` row, call `fail_stale_installs` → `failed`, error `server restarted`.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p coppice-server --lib plugins::git_install` and `cargo test -p coppice-server --features embedded-test-db --test integration_plugins install`
Expected: FAIL.

- [ ] **Step 4: Implement** per the interfaces above.

- [ ] **Step 5: Run tests**

Run: same commands as Step 3.
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add server
git commit -m "feat(m10): install and update plugins from git"
```

---

### Task 7: Web — Settings → Plugins

**Files:**
- Create: `web/src/lib/schemas/plugin.ts`, `web/src/features/plugins/usePlugins.ts`, `web/src/features/plugins/PluginsPage.tsx`, `web/src/features/plugins/PluginsPage.test.tsx`
- Modify: `web/src/App.tsx` (route `/settings/plugins`), `web/src/components/AppShell.tsx` (nav item `{ to: '/settings/plugins', label: 'Plugins', icon: Puzzle, adminOnly: true }` after Repositories)

**Interfaces:**
- Consumes: Task 4–6 API shapes (camelCase).
- Produces: `PluginDir`, `Plugin`, `PluginSkill`, `PluginInstall` TS types; hooks `usePluginDirs()`, `usePlugins()`, `useAddPluginDir()`, `useMovePluginDir()`, `useRemovePluginDir()`, `useRescanPlugins()`, `useSetPluginEnabled()`, `useInstallPlugin()`, `useUpdatePlugin()`, `usePluginInstall(id)` (polls every 2 s while `status === 'running'`, then invalidates `['plugins']`). Query keys `['plugin-dirs']`, `['plugins']`, `['plugin-installs', id]`. `usePlugins` is reused by Task 8.

Page layout (follow `RepositoriesPage.tsx` and `docs/web/DESIGN.md`):
- **Plugin directories:** ordered list with up/down, remove (hidden for default), "Add directory" (path input + Browse via `pickDirectory()` when `isDesktopShell()`), Rescan button.
- **Install from git:** git URL, optional ref, target directory select, Install; shows running/failed state with the git error.
- **Plugin cards:** name, version, source (+ short commit for git), status badge (`ok`/`invalid`/`missing`/`shadowed`), error text, skill count, MCP server count, "Not supported yet: commands, hooks" when `unsupported` non-empty, enable toggle (disabled unless `status === 'ok'`), Update (git only). Expanding a card lists skills with descriptions and skill errors.

- [ ] **Step 1: Write failing tests** in `PluginsPage.test.tsx` (mock `apiFetch` like `RepositoriesPage.test.tsx`):
  - `renders plugin cards with status and unsupported parts` — shows `sample-plugin`, `1.2.0`, `Not supported yet: commands, hooks`.
  - `enable toggle is disabled for shadowed plugins` — a `shadowed` plugin's toggle is disabled.
  - `toggling enable sends PATCH` — click → `PATCH /api/plugins/<id>` with `{"enabled":true}`.
  - `install posts git url and shows failure` — submit → `POST /api/plugins/install` body `{gitUrl, ref, pluginDirId}`; install poll returns `failed` with `error: "repository not found"` → text visible.
  - `default directory cannot be removed` — no remove button on the `isDefault` row.

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && yarn test src/features/plugins`
Expected: FAIL.

- [ ] **Step 3: Implement** schemas, hooks, page, route, nav.

- [ ] **Step 4: Run tests**

Run: `make web-test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src
git commit -m "feat(m10): Settings → Plugins page"
```

---

### Task 8: Web — agent form Plugins picker, docs

**Files:**
- Modify: `web/src/features/agents/AgentForm.tsx`, `web/src/features/agents/AgentsPage.tsx`, `web/src/features/agents/useAgents.ts`, `docs/architecture.md`, `docs/development.md`, `docs/milestones/M10-plugins.md`
- Create: `web/src/features/agents/AgentForm.test.tsx` (if absent; else extend)

**Interfaces:**
- Consumes: `usePlugins()` (Task 7), `GET/PUT /api/agents/{id}/plugins` (Task 5).
- Produces: `AgentFormValues.pluginIds: string[]`; `useAgentPlugins(agentId)` and `useSetAgentPlugins()` in `useAgents.ts`.

- [ ] **Step 1: Write failing tests** in `AgentForm.test.tsx`:
  - `lists only enabled ok plugins` — plugins `[ok+enabled A, ok+disabled B, shadowed+enabled C]` → only A is offered.
  - `submits selected plugin ids` — check A, submit → `onSubmit` receives `pluginIds: [A.id]`.
  - `unavailable assigned plugin is flagged and dropped on save` — agent assigned to C → C rendered disabled with the text "will be removed on save", and `onSubmit` receives `pluginIds` without C (the server rejects non-enabled / non-ok ids).

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && yarn test src/features/agents`
Expected: FAIL.

- [ ] **Step 3: Implement.** Multi-select checkbox list under Skills; `AgentsPage` calls `useSetAgentPlugins` with the new agent id after create/update succeeds.

- [ ] **Step 4: Docs.** `docs/architecture.md`: plugins module/service split, catalog + token snapshot, per-run OpenCode serve. `docs/development.md`: plugin dirs (`[plugins] dir`, Docker `/data/plugins`), installing from git, `allow_file_git_urls`. `docs/milestones/M10-plugins.md`: tick "Plugin dirs, scan, git install, enable, agent assignment work via UI and API" and "Claude Code / Cursor format plugins and skills-only folders load unchanged; unsupported parts listed".

- [ ] **Step 5: Final verification**

Run: `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test`, then `make compose-up && make e2e-smoke-m03 && make e2e-smoke-m09`.
Expected: all pass; then `make clean`.

- [ ] **Step 6: Commit**

```bash
git add web/src docs
git commit -m "feat(m10): agent plugin picker; docs for Part 2a"
```
