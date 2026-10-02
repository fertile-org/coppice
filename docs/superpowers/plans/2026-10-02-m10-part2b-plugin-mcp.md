# M10 Part 2b — Plugin MCP Proxy and Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Proxy plugin MCP servers (stdio + streamable HTTP) through the Coppice gateway as a third `ToolSource`, with encrypted plugin settings, a Test button, a per-run tool-call log, and gateway tool names rendered in every live console.

**Architecture:** Pure placeholder resolution feeds an `McpServerPool` of shared server instances; each instance talks through an `McpTransport` implementation built on the `rmcp` client. `PluginMcpSource` lists and calls pool tools for the token's plugin snapshot; `ToolRegistry` bounds every source's `list`. Settings live in `plugin_settings` over the M07 `SecretService`. The web adds settings/Test/warning to the plugin card and a Tools & Skills tab to the run row; consoles split gateway names via the connector descriptor's `ToolNameStyle`.

**Tech Stack:** Rust (Axum, SQLx, tokio, `rmcp` 3.5 client, async-trait, sha2), React + TanStack Query + zod + Vitest, embedded Postgres tests, Node smoke scripts.

**Spec:** [docs/superpowers/specs/2026-10-02-m10-part2b-plugin-mcp-design.md](../specs/2026-10-02-m10-part2b-plugin-mcp-design.md) (parent: [M10 Plugins Design](../specs/2026-09-29-m10-plugins-design.md); builds on [foundations](../specs/2026-10-01-connector-and-tool-source-foundations-design.md)).

## Global Constraints

- **No secret leaves the server.** Plugin API responses, Test results, tool results, logs, and error messages never contain MCP `command`, `args`, `env`, `url`, `headers`, or any plugin setting value. Errors name keys (`missing setting "VAR"`), never values.
- Server-environment fallback for `${VAR}` excludes names starting with `COPPICE_` and exactly `DATABASE_URL`, `SECRETS_MASTER_KEY`. stdio children: environment cleared, then `PATH`, `HOME`, `LANG`, `TMPDIR` from the server process when set, then the entry's resolved `env`; cwd = plugin root.
- Exposed plugin tool names: `<plugin>__<tool>`, parts sanitized to `[A-Za-z0-9_-]`, max 50 chars (41 chars + `_` + 8 hex of SHA-256 of the unsanitized `<plugin>__<tool>` when longer).
- Config `[plugins]` defaults: `mcp_start_timeout_secs = 20`, `mcp_list_timeout_secs = 10`, `mcp_idle_shutdown_secs = 600`. Backoff 1 s doubling to 60 s; unhealthy after 3 consecutive failures.
- All plugin mutations (`settings`, `test`) are admin-only (`AdminUser`) and CSRF-protected; reads use `AuthUser`. Handlers stay thin; logic in services / `mcp::proxy`.
- No connector-id literals outside `providers/` and `sessions/opencode*` (the `no_connector_literals_outside_providers` scan); the gateway server name comes from `protocol::SERVER_NAME`.
- Plugin MCP server name in tests and smoke comes from fixtures; tests never need network access beyond loopback. Agent tests use `MockProvider`.
- Iterate with `make test-unit`, targeted `cargo test -p coppice-server --features embedded-test-db <target> -- <filters>` (filters after `--`), `cargo test -p coppice-connectors`, `cd web && yarn test <path>`. `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test` at the end (Task 9).
- Format only files you touch (never repo-wide `cargo fmt`). Pre-existing `yarn lint` errors are not yours. Default Docker Compose stack only; never `docker compose down -v`.

## Review Focus

1. **A plugin server that hangs on start or `tools/list`** — a run's `tools/list` still returns within the list timeout with every core and skill tool. (Task 4: `hung_source_list_times_out_core_tools_intact`, `plugin_source_hung_server_does_not_block_list`.)
2. **Secret-bearing values** — a setting value and a header value never appear in `GET /api/plugins`, the Test response, or a failing server's error. (Task 1: `settings_values_never_returned`; Task 5: `test_response_has_no_secret_values`.)
3. **Server crashes mid-run** — the call returns `plugin "<name>" unavailable`, the next call after backoff restarts the server, three consecutive failures hide its tools, and Test brings it back. (Task 3: `crash_then_backoff_restart`, `three_failures_unhealthy_hides_tools`, `test_clears_unhealthy`.)
4. **Chat and compaction runs** — a plugin tool without `readOnlyHint` is neither listed nor callable in `human_chat`/`conversation`; `knowledge_compaction` sees no plugin tools. (Task 4: `chat_profile_lists_only_read_only_plugin_tools`, `compaction_lists_no_plugin_tools`.)
5. **Plugin disabled after the run's token was minted** — its tools disappear from the next `tools/list`, calls are denied, and its server is stopped. (Task 4: `disabled_plugin_tools_vanish_mid_run`; Task 5: `disable_stops_plugin_servers`.)

---

### Task 1: Plugin settings and placeholder resolution

**Files:**
- Create: `server/migrations/030_plugin_settings.sql`, `server/src/plugins/placeholders.rs`, `server/src/services/plugin_settings_service.rs`
- Modify: `server/src/plugins/mod.rs`, `server/src/services/mod.rs`, `server/src/api/plugins.rs` (route + `settings` in `PluginResponse`)
- Test: unit tests in `placeholders.rs`; `server/tests/integration_plugins.rs`

**Interfaces:**
- Consumes: `McpServerEntry`, `McpServerTransport` (`plugins::capability`); `SecretService::{upsert_named, decrypt_by_id, delete_by_id}`; `PluginService::get_plugin`.
- Produces:
  - `plugins::placeholders::placeholder_keys(entries: &[McpServerEntry]) -> BTreeSet<String>` (excludes `CLAUDE_PLUGIN_ROOT`; scans command, args, env values, url, header values).
  - `pub enum ResolvedTransport { Stdio { command: String, args: Vec<String>, env: BTreeMap<String, String> }, Http { url: String, headers: BTreeMap<String, String> } }` with `fn kind(&self) -> &'static str` (`"stdio"` / `"http"`).
  - `pub struct ResolveCtx<'a> { pub plugin_root: &'a Path, pub settings: &'a BTreeMap<String, String>, pub env: &'a dyn Fn(&str) -> Option<String> }`.
  - `pub fn resolve(transport: &McpServerTransport, ctx: &ResolveCtx) -> Result<ResolvedTransport, String>` — `Unsupported { kind }` → `Err(format!("unsupported transport \"{kind}\""))`.
  - `pub fn server_env_allowed(name: &str) -> bool`.
  - `PluginSettingsService::new(pool: &PgPool, store: &SecretStore)`; `configured_keys(plugin_id) -> Result<BTreeSet<String>, PluginSettingsError>`; `set(plugin_id, allowed: &BTreeSet<String>, values: BTreeMap<String, String>) -> Result<(), PluginSettingsError>` (empty value clears); `decrypted(plugin_id) -> Result<BTreeMap<String, String>, PluginSettingsError>`. `PluginSettingsError::{UnknownKey(String), Secret(SecretError), Db(sqlx::Error)}`.
  - API: `PUT /api/plugins/{plugin_id}/settings` body `{ "values": { "<KEY>": "<value>" } }` → `PluginResponse`; `PluginResponse.settings: Vec<PluginSettingResponse { key, configured }>` (camelCase JSON), sorted by key.

- [ ] **Step 1: Write failing tests**
  - Unit (`placeholders.rs`):
    - `resolves_plugin_root_settings_and_env` — `${CLAUDE_PLUGIN_ROOT}/bin/fs` → `/p/bin/fs`; `${API_TOKEN}` from settings; `${HOME_DIR}` from env when no setting; setting wins over env.
    - `blocked_env_names_are_missing` — `${COPPICE_SECRETS__MASTER_KEY}`, `${DATABASE_URL}`, `${SECRETS_MASTER_KEY}` with env present → `Err("missing setting \"<NAME>\"")`.
    - `default_value_syntax` — `${PORT:-8080}` → `8080` when absent; setting `PORT=9` → `9`.
    - `unterminated_placeholder_kept_verbatim` — `"a${b"` → `"a${b"`.
    - `unsupported_transport_errors` — `Unsupported { kind: "sse" }` → `Err("unsupported transport \"sse\"")`.
    - `placeholder_keys_collects_all_fields` — keys from command, args, env values, url, header values; `CLAUDE_PLUGIN_ROOT` excluded; `${X:-d}` yields `X`.
    - `error_never_contains_values` — a failing resolve with settings `{API_TOKEN: "s3cr3t"}` → error string lacks `s3cr3t`.
  - Integration (`integration_plugins.rs`, fixture `fixtures/plugins/inline-mcp`, keys `API_TOKEN`, `ROOT`):
    - `settings_put_marks_configured` — admin PUT `{API_TOKEN: "tok"}` → 200; response `settings == [{key:"API_TOKEN",configured:true},{key:"ROOT",configured:false}]`; DB has one `plugin_settings` row and one `secrets` row named `plugin-setting-<id>-API_TOKEN`.
    - `settings_empty_value_clears` — PUT `{API_TOKEN: ""}` → configured false; secret row deleted.
    - `settings_unknown_key_rejected` — PUT `{NOPE: "x"}` → 400 `unknown setting "NOPE"`.
    - `settings_values_never_returned` — after PUT `{API_TOKEN: "s3cr3t-value"}`, bodies of `GET /api/plugins`, `GET /api/plugins/{id}`, and the PUT response do not contain `s3cr3t-value`.
    - `settings_requires_admin` — member → 403; missing CSRF → 403.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --lib plugins::placeholders` and `--test integration_plugins -- settings_` → FAIL.
- [ ] **Step 3: Implement** the migration (`plugin_settings (plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE, key TEXT NOT NULL, secret_id UUID NOT NULL REFERENCES secrets(id) ON DELETE CASCADE, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (plugin_id, key))`), the resolver, the service (secret name `plugin-setting-<plugin_id>-<key>`), the route (allowed keys = `placeholder_keys(manifest.mcp_servers)`), and `settings` in every `PluginResponse`.
- [ ] **Step 4: Run** the same commands → PASS; `--test integration_plugins` (whole file) → PASS.
- [ ] **Step 5: Commit** `feat(plugins): encrypted plugin settings; placeholder resolution`

### Task 2: `McpTransport` seam; stdio and HTTP on `rmcp`; `fake-mcp`

**Files:**
- Create: `server/src/mcp/proxy/mod.rs`, `server/src/mcp/proxy/transport.rs`, `server/src/mcp/proxy/stdio.rs`, `server/src/mcp/proxy/http.rs`, `server/tests/support/fake_mcp.rs`, `server/tests/integration_mcp_transport.rs`
- Modify: `server/Cargo.toml` (`rmcp = { version = "3.5", default-features = false, features = ["client", "transport-child-process", "transport-streamable-http-client-reqwest", "reqwest"] }`; `[[bin]] name = "fake-mcp" path = "tests/support/fake_mcp.rs" required-features = ["embedded-test-db"]`), `server/src/mcp/mod.rs`

**Interfaces:**
- Consumes: `ResolvedTransport` (Task 1); `ToolResult`, `ToolContent` (`mcp::protocol`).
- Produces (`mcp::proxy`):
  - `pub struct RemoteTool { pub name: String, pub description: String, pub input_schema: Value, pub read_only: bool }` (`read_only` = `annotations.readOnlyHint == true`).
  - `pub enum ProxyError { Start(String), Protocol(String), Closed, Timeout }` (+ `Display`).
  - `#[async_trait] pub trait McpConnection: Send + Sync { async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError>; async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult, ProxyError>; fn take_tools_changed(&self) -> bool; fn is_closed(&self) -> bool; async fn close(&self); }`
  - `#[async_trait] pub trait McpTransport: Send + Sync { fn kind(&self) -> &'static str; async fn connect(&self, spec: &ResolvedTransport, cwd: &Path) -> Result<Box<dyn McpConnection>, ProxyError>; }`
  - `pub struct Transports` with `builtin() -> Self` (stdio + http), `register(Arc<dyn McpTransport>)`, `get(kind: &str) -> Option<Arc<dyn McpTransport>>`.
  - `StdioTransport`, `HttpTransport`.
- `fake-mcp` (hand-written newline-delimited JSON-RPC over stdio; no rmcp server features): methods `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. Tools: `echo` (`readOnlyHint: true`, returns `args.text`), `write_note` (no annotation, returns `"noted"`), `env` (`readOnlyHint: true`, returns value of env var `args.name` or `"<unset>"`), `sleep` (sleeps `args.ms`, returns `"slept"`), `crash` (exits 1). Env switches: `FAKE_MCP_START_FAIL=1` (exit 1 before reading), `FAKE_MCP_TOOLS_CHANGED=1` (after the first `tools/call`, add tool `extra` and send `notifications/tools/list_changed`), `FAKE_MCP_PID_FILE=<path>` (write own pid, relative to cwd), `FAKE_MCP_INIT_SLEEP_MS=<n>` (sleep before answering `initialize`).

- [ ] **Step 1: Write failing tests** in `server/tests/integration_mcp_transport.rs` (top `#![cfg(feature = "embedded-test-db")]`; command `env!("CARGO_BIN_EXE_fake-mcp")`):
  - `stdio_lists_and_calls` — tools are `echo`, `write_note`, `env`, `sleep`, `crash` with `echo.read_only == true`, `write_note.read_only == false`; `call_tool("echo", {"text":"hi"})` → `ToolResult { content: [Text("hi")], is_error: false }`.
  - `stdio_env_is_minimal` — server process has `SOME_SERVER_SECRET=x`; `env {name:"SOME_SERVER_SECRET"}` → `"<unset>"`; `env {name:"PATH"}` ≠ `"<unset>"`; resolved env `{"GREETING":"hey"}` → `env {name:"GREETING"}` → `"hey"`.
  - `stdio_cwd_is_plugin_root` — `connect` with cwd = a tempdir and resolved env `FAKE_MCP_PID_FILE=pid.txt` (relative) → `<tempdir>/pid.txt` exists after `initialize`.
  - `stdio_crash_marks_closed` — `call_tool("crash")` → `Err(Closed | Protocol(_))`; afterwards `is_closed() == true`.
  - `stdio_start_failure_is_start_error` — `FAKE_MCP_START_FAIL=1` → `connect` → `Err(ProxyError::Start(_))`.
  - `stdio_tools_changed_flag` — with `FAKE_MCP_TOOLS_CHANGED=1`, after one call, within 2 s `take_tools_changed() == true` and `list_tools` includes `extra`; a second `take_tools_changed()` is `false`.
  - `http_lists_and_calls_with_headers` — a loopback axum JSON-RPC stub (in the test file) that requires header `Authorization: Bearer h-val`; `connect(Http { url, headers })` → `list_tools` returns its one tool, `call_tool` returns text.
  - `http_error_hides_header_value` — stub returns 401; `connect`/`list_tools` error `Display` lacks `h-val`.
  - `close_kills_child` — with `FAKE_MCP_PID_FILE`, after `close()` the pid is gone from `/proc` within 2 s.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp_transport` → FAIL.
- [ ] **Step 3: Implement** the seam and both transports (child `kill_on_drop`; stderr lines → `tracing::debug!(target: "coppice::plugin_mcp", …)`; client handler sets the tools-changed flag on `tools/list_changed`; rmcp content text/image mapped, other content → `Text("[unsupported content: <type>]")`), and `fake-mcp`. If `rmcp` feature names differ in 3.5, use the equivalent client features and note it in the report.
- [ ] **Step 4: Run** the same command → PASS; `cargo build -p coppice-server --release` succeeds (Docker builder is Rust 1.88).
- [ ] **Step 5: Commit** `feat(mcp): McpTransport seam with stdio and HTTP rmcp clients`

### Task 3: `McpServerPool`

**Files:**
- Create: `server/src/mcp/proxy/pool.rs`
- Modify: `server/src/mcp/proxy/mod.rs`, `config/src/lib.rs` (`PluginsConfig` fields), `config.example.toml`, `deploy/config/config.example.toml` (commented `[plugins]` keys)
- Test: unit tests in `pool.rs` (fake transport), `server/tests/integration_mcp_transport.rs` (real `fake-mcp`)

**Interfaces:**
- Consumes: `Transports`, `McpConnection`, `RemoteTool`, `ProxyError` (Task 2); `resolve`, `ResolveCtx`, `server_env_allowed` (Task 1); `McpServerEntry`.
- Produces:
  - `PluginsConfig { …, mcp_start_timeout_secs: u64 = 20, mcp_list_timeout_secs: u64 = 10, mcp_idle_shutdown_secs: u64 = 600 }`.
  - `#[derive(Clone, Hash, Eq, PartialEq)] pub struct ServerKey { pub plugin_id: Uuid, pub server: String }`.
  - `#[derive(Clone)] pub struct PoolServerSpec { pub key: ServerKey, pub plugin_name: String, pub plugin_root: PathBuf, pub entry: McpServerEntry, pub settings: BTreeMap<String, String> }`.
  - `pub enum ServerHealth { Stopped, Starting, Ready, Backoff, Unhealthy }` with `fn as_str(&self) -> &'static str` (`stopped|starting|ready|backoff|unhealthy`).
  - `pub enum PoolError { Unavailable, Unhealthy(String), Config(String), Timeout }` — `Config` = placeholder/unsupported error text (key names only).
  - `pub struct PoolConfig { pub start_timeout: Duration, pub idle_shutdown: Duration, pub backoff_initial: Duration, pub backoff_max: Duration, pub unhealthy_after: u32 }` with `PoolConfig::from_plugins(&PluginsConfig)` (1 s, 60 s, 3).
  - `McpServerPool::new(transports: Transports, cfg: PoolConfig) -> Self`; `async fn tools(&self, spec: &PoolServerSpec) -> Result<Vec<RemoteTool>, PoolError>`; `async fn call(&self, spec: &PoolServerSpec, tool: &str, args: Value) -> Result<ToolResult, PoolError>`; `async fn test(&self, spec: &PoolServerSpec) -> Result<Vec<RemoteTool>, PoolError>` (stop, clear failures, start fresh); `fn health(&self, key: &ServerKey) -> ServerHealth`; `async fn stop_plugin(&self, plugin_id: Uuid)`; `async fn reap_idle(&self)`; `fn spawn_reaper(self: &Arc<Self>) -> tokio::task::JoinHandle<()>` (ticks every 30 s).
  - Resolution env for `ResolveCtx.env`: `std::env::var` filtered by `server_env_allowed`.

- [ ] **Step 1: Write failing tests**
  - Unit (`pool.rs`, a `FakeTransport` counting connects whose connections can be told to fail/close):
    - `lazy_start_once_for_concurrent_callers` — 5 concurrent `tools()` → 1 connect.
    - `tools_cached_until_changed` — two `tools()` → one `list_tools`; after `take_tools_changed` returns true, next `tools()` re-lists.
    - `crash_then_backoff_restart` — connection closes on `call` → `Err(Unavailable)`, health `Backoff`; a call before 1 s → `Err(Unavailable)` without connect; after backoff (use `tokio::time::pause`/`advance`) → reconnects and succeeds.
    - `three_failures_unhealthy_hides_tools` — 3 failed starts → health `Unhealthy`; `tools()` → `Err(Unhealthy(_))` without another connect.
    - `test_clears_unhealthy` — after unhealthy, `test()` with a now-working transport → `Ok(tools)`, health `Ready`.
    - `config_error_is_unhealthy_immediately` — missing setting → `Err(Config("missing setting \"API_TOKEN\""))`, health `Unhealthy`, zero connects.
    - `fingerprint_change_restarts` — same key, different settings value → old connection closed, new connect.
    - `idle_reaped` — after `idle_shutdown` with no use, `reap_idle()` closes it; health `Stopped`.
    - `stop_plugin_closes_all_its_servers` — two servers of one plugin + one of another; `stop_plugin(a)` closes only a's.
    - `start_continues_after_caller_timeout` — slow connect (3 s); a caller wrapping `tools()` in a 1 s timeout gives up; after 3 s health is `Ready` without a second connect.
  - Integration (`integration_mcp_transport.rs`, real `fake-mcp`):
    - `pool_shares_one_process_across_callers` — two `call("echo")` via the pool → one pid in `FAKE_MCP_PID_FILE` writes.
    - `pool_restarts_crashed_stdio_server` — `call("crash")` → `Unavailable`; after backoff `call("echo")` → ok with a new pid.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --lib mcp::proxy::pool` and `--test integration_mcp_transport -- pool_` → FAIL.
- [ ] **Step 3: Implement** the pool: per-key state behind a mutex; start in a spawned task bounded by `start_timeout` whose result all waiters share; failures counted per key; fingerprint = SHA-256 over the serialized `ResolvedTransport`.
- [ ] **Step 4: Run** the same commands → PASS; `cargo test -p coppice-config` → PASS.
- [ ] **Step 5: Commit** `feat(mcp): shared plugin MCP server pool with backoff, health, idle shutdown`

### Task 4: `PluginMcpSource`; bounded source listing

**Files:**
- Create: `server/src/mcp/proxy/naming.rs`, `server/src/mcp/proxy/source.rs`
- Modify: `server/src/mcp/registry.rs` (list timeout), `server/src/lib.rs` (`AppState.plugin_mcp`, registry construction), `server/src/main.rs`, `server/tests/common/mod.rs`, `server/tests/integration_auth.rs` (literal `AppState` builds), `server/src/mcp/proxy/mod.rs`
- Test: unit tests in `naming.rs`, `registry.rs`, `source.rs`; `server/tests/integration_mcp.rs`; fixture plugin `fixtures/plugins/mcp-fake/` (`.claude-plugin/plugin.json` `{"name":"mcp-fake"}`, `.mcp.json` with stdio server `fake` whose `command` is `${FAKE_MCP_BIN}`), `fixtures/plugins/mcp-fake-slow/` (name `mcp-fake-slow`, same server plus `env` `FAKE_MCP_INIT_SLEEP_MS: "5000"`); mock fixtures `fixtures/agent-responses/mcp/plugin_mcp_tool_call.json` (`toolCalls`: `mcp-fake__echo {"text":"hi"}`, then `result_submit`) and `fixtures/agent-responses/mcp/chat_plugin_write_call.json` (`toolCalls`: `mcp-fake__write_note {}`, then `result_submit` reply)

**Interfaces:**
- Consumes: `McpServerPool`, `PoolServerSpec`, `ServerKey`, `PoolError`, `RemoteTool` (Task 3); `PluginSettingsService::decrypted` (Task 1); `ToolSource`, `SourcedTool`, `SourceKind::Plugin`, `ToolRegistry`, `RunToolScope`, `ToolDefinition`.
- Produces:
  - `naming::exposed_name(plugin: &str, tool: &str) -> String`.
  - `#[async_trait] pub trait PluginServerCatalog: Send + Sync { async fn servers_for(&self, plugin_ids: &[Uuid]) -> Vec<PoolServerSpec>; }` and `DbPluginServerCatalog::new(pool: PgPool, store: SecretStore)` (enabled `ok` plugins in `plugin_ids`, root = dir path + rel_path, settings decrypted, one spec per `mcp_servers` entry).
  - `PluginMcpSource::new(pool: Arc<McpServerPool>, catalog: Arc<dyn PluginServerCatalog>)`; `SourcedTool.key = "<server>/<remote tool>"`, `plugin_id = Some(id)`.
  - `ToolRegistry::with_list_timeout(self, d: Duration) -> Self`; `pub const DEFAULT_LIST_TIMEOUT: Duration = 10 s`.
  - `AppState.plugin_mcp: Arc<McpServerPool>`; `AppState::build_tool_registry(db: Option<&PgPool>, secret_store: &SecretStore, plugin_mcp: Arc<McpServerPool>, list_timeout: Duration) -> Arc<ToolRegistry>` replacing `builtin_tool_registry()` (no plugin source when `db` is `None`). `main.rs` spawns `plugin_mcp.spawn_reaper()`.

- [ ] **Step 1: Write failing tests**
  - Unit:
    - `exposed_name_basic_and_sanitized` — `("github","create_issue")` → `github__create_issue`; `("my.plugin","do it")` → `my_plugin__do_it`.
    - `exposed_name_long_is_hashed_to_50` — 60-char input → length 50, first 41 chars kept, char 42 is `_`, last 8 are lowercase hex; two inputs sharing the first 41 chars give different names.
    - `hung_source_list_times_out_core_tools_intact` (registry) — a source whose `list` sleeps 30 s, registry with 100 ms list timeout → `tools_for` returns within 1 s with all core tools for `Full`.
    - `compaction_lists_no_plugin_tools` (source) — `KnowledgeCompaction` scope → empty, catalog not queried.
    - `unavailable_call_returns_tool_error` (source) — pool `Unavailable` → `Ok(ToolResult { is_error: true, content: [Text("plugin \"mcp-fake\" unavailable")] })`; `Config("missing setting \"X\"")` → text `plugin "mcp-fake" unavailable: missing setting "X"`.
  - Integration (`integration_mcp.rs`, `bootstrap_and_login_with_gateway`, rescan the `mcp-fake` fixture dir, set setting `FAKE_MCP_BIN` to `env!("CARGO_BIN_EXE_fake-mcp")`, enable, assign to the agent):
    - `plugin_mcp_tool_listed_and_called` — run with `plugin_mcp_tool_call.json` finishes done; `tools/list` (minted token with the plugin id) includes `mcp-fake__echo` with `annotations.readOnlyHint == true`; `run_tool_calls` row `tool = 'mcp-fake__echo'`, `source = 'plugin'`, `plugin_id = <id>`, `status = 'ok'`.
    - `chat_profile_lists_only_read_only_plugin_tools` — `HumanChat` token lists `mcp-fake__echo` and `mcp-fake__env`, not `mcp-fake__write_note`; chat turn with `chat_plugin_write_call.json` logs `mcp-fake__write_note` as `denied`.
    - `plugin_source_hung_server_does_not_block_list` — registry list timeout 300 ms, plugin server started with `FAKE_MCP_INIT_SLEEP_MS=5000` (fixture `fixtures/plugins/mcp-fake-slow/`, same as `mcp-fake` plus that env entry) → `tools/list` returns within 2 s with core tools.
    - `disabled_plugin_tools_vanish_mid_run` — token minted with the plugin id; disable plugin; `tools/list` lacks `mcp-fake__*`; `tools/call mcp-fake__echo` → `denied: tool "mcp-fake__echo" is not available for this run`.
    - `unassigned_plugin_tools_not_listed` — plugin enabled but not in the token snapshot → no `mcp-fake__*`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --lib -- mcp::proxy mcp::registry` and `--test integration_mcp -- plugin_ chat_profile_lists_only disabled_plugin unassigned_plugin` → FAIL.
- [ ] **Step 3: Implement** naming, catalog, source, registry timeout (each source's `list` wrapped in `tokio::time::timeout`; timeout → no tools from that source + `tracing::warn!`), and `AppState` wiring (list timeout from `config.plugins.mcp_list_timeout_secs`).
- [ ] **Step 4: Run** the same commands → PASS; `--test integration_mcp` and `--test integration_chat` (whole files) → PASS.
- [ ] **Step 5: Commit** `feat(mcp): plugin MCP tool source; bounded source listing`

### Task 5: Test endpoint, server health in responses, lifecycle hooks

**Files:**
- Modify: `server/src/api/plugins.rs` (route, `McpServerResponse.health`, disable/update hooks), `server/src/services/plugin_service.rs` (helper to build `PoolServerSpec`s for one plugin regardless of enabled)
- Test: `server/tests/integration_plugins.rs`

**Interfaces:**
- Consumes: `McpServerPool::{test, health, stop_plugin}`, `PoolServerSpec`, `ServerHealth`, `exposed_name`, `PluginSettingsService::decrypted`.
- Produces:
  - `POST /api/plugins/{plugin_id}/test` → `{ "servers": [{ "name", "kind", "status": "ok"|"error"|"unsupported", "error"?, "tools": [{ "name", "exposedName", "description", "readOnly" }] }] }` (servers in manifest order; `unsupported` for `Unsupported` transports, with no tools). 404 unknown plugin; 409 `plugin is not ok` when status ≠ `ok`.
  - `McpServerResponse { name, kind, health }` (`health` = `ServerHealth::as_str`).
  - `PluginService::server_specs(plugin_id, store: &SecretStore) -> Result<Vec<PoolServerSpec>, PluginError>`.

- [ ] **Step 1: Write failing tests** (fixture `mcp-fake` from Task 4, plus `sample-plugin` for unsupported):
  - `test_lists_tools_while_disabled` — plugin disabled, setting `FAKE_MCP_BIN` set → 200; `servers[0].status == "ok"`; tools include `{name:"echo", exposedName:"mcp-fake__echo", readOnly:true}`; afterwards `GET /api/plugins/{id}` shows `mcpServers[0].health == "ready"`.
  - `test_reports_missing_setting` — no `FAKE_MCP_BIN` → `status == "error"`, `error == "missing setting \"FAKE_MCP_BIN\""`.
  - `test_marks_unsupported` — `sample-plugin` server `old` (sse) → `status == "unsupported"`, `tools == []`.
  - `test_response_has_no_secret_values` — HTTP entry fixture (`fixtures/plugins/mcp-http/` with header `Authorization: Bearer ${API_KEY}` pointing at a closed loopback port) and setting `API_KEY = "hdr-s3cr3t"` → Test `status == "error"`; response body contains neither `hdr-s3cr3t` nor the `command`/`url`/`headers` keys.
  - `test_requires_admin` — member → 403.
  - `disable_stops_plugin_servers` — after a Test (health `ready`), PATCH disable → `GET` shows `health == "stopped"`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_plugins -- test_ disable_stops` → FAIL.
- [ ] **Step 3: Implement** the route (thin: load specs → `pool.test` per supported server → map), `health` in responses, and `stop_plugin` on disable and on git update completion.
- [ ] **Step 4: Run** the same command → PASS; `--test integration_plugins` (whole file) → PASS.
- [ ] **Step 5: Commit** `feat(plugins): MCP Test endpoint, server health, stop on disable/update`

### Task 6: Web — plugin settings, Test, stdio warning, health

**Files:**
- Modify: `web/src/lib/schemas/plugin.ts`, `web/src/features/plugins/usePlugins.ts`, `web/src/features/plugins/PluginCard.tsx`
- Create: `web/src/features/plugins/PluginSettingsForm.tsx`, `web/src/features/plugins/PluginTestResults.tsx`
- Test: `web/src/features/plugins/PluginCard.test.tsx` (create if absent), `web/src/features/plugins/PluginsPage.test.tsx`

**Interfaces:**
- Consumes: Task 1 and Task 5 API shapes.
- Produces: `pluginSettingSchema { key: string, configured: boolean }`; `pluginSchema.settings` (default `[]`); `pluginMcpServerSchema.health` (enum `stopped|starting|ready|backoff|unhealthy`, `.catch('stopped')`); `pluginTestResultSchema`; hooks `useSetPluginSettings(pluginId)` (PUT, invalidates `PLUGINS_QUERY_KEY`) and `useTestPlugin(pluginId)` (POST, invalidates `PLUGINS_QUERY_KEY`).

- [ ] **Step 1: Write failing tests** (Vitest + Testing Library, mocked fetch):
  - `renders settings with configured badges` — keys `API_TOKEN` (configured) and `ROOT` (not) render password inputs; `API_TOKEN` shows "Configured".
  - `saves a setting` — type in `ROOT`, Save → PUT `/api/plugins/<id>/settings` body `{"values":{"ROOT":"v"}}` with `X-CSRF-Token`.
  - `clears a setting` — Clear on `API_TOKEN` → body `{"values":{"API_TOKEN":""}}`.
  - `test button shows tools and errors` — Test → list shows `mcp-fake__echo` (read-only badge) and an error row `missing setting "X"`.
  - `enabling plugin with stdio server asks for confirmation` — `window.confirm` called with exactly "This plugin starts local MCP servers that run with the Coppice server's privileges until sandboxing lands (M11). Enable anyway?"; returning false sends no PATCH; a plugin with only http servers does not confirm.
  - `shows server health` — `health: "unhealthy"` renders "unhealthy" next to the server name.
  - `unknown health falls back` — `health: "weird"` parses as `stopped`.
- [ ] **Step 2: Run** `cd web && yarn test src/features/plugins` → FAIL.
- [ ] **Step 3: Implement** schemas, hooks, components; settings and Test sections are admin-only in the card (match how the enable toggle is gated today).
- [ ] **Step 4: Run** `cd web && yarn test src/features/plugins` → PASS.
- [ ] **Step 5: Commit** `feat(web): plugin settings, MCP Test, stdio warning, server health`

### Task 7: Tool-calls API and Tools & Skills tab

**Files:**
- Create: `server/src/services/run_tool_call_service.rs`, `web/src/lib/schemas/toolCall.ts`, `web/src/features/runs/useRunToolCalls.ts`, `web/src/features/runs/RunToolsAndSkills.tsx`
- Modify: `server/src/services/mod.rs`, `server/src/api/agent_runs.rs` (route), `web/src/features/tickets/TicketRunsTab.tsx` (details tabs)
- Test: `server/tests/integration_mcp.rs` (or `integration_agents.rs` if run fixtures live there), `web/src/features/runs/RunToolsAndSkills.test.tsx`, `web/src/features/tickets/TicketRunsTab.test.tsx`

**Interfaces:**
- Produces:
  - `RunToolCallService::new(pool)`; `list_for_run(run_id) -> Result<RunToolCalls { items: Vec<RunToolCallRow>, skills_used: Vec<String> }, sqlx::Error>`; `RunToolCallRow { id, tool, source, plugin_id, plugin_name: Option<String> (LEFT JOIN plugins), status, error, duration_ms, args_summary, created_at }`, ordered `created_at, id`. `skills_used` = distinct `name` parsed from `args_summary` JSON of `tool = 'skill_load' AND status = 'ok'` rows, first-use order (unparseable summaries skipped).
  - `GET /api/agent-runs/{run_id}/tool-calls` (`AuthUser`) → `{ items: [...camelCase...], skillsUsed: [...] }`; 404 `agent run not found`.
  - Web: `runToolCallSchema`, `runToolCallsSchema`; `useRunToolCalls(runId: string, enabled: boolean)` query key `['agent-run-tool-calls', runId]`; `<RunToolsAndSkills runId enabled />`.

- [ ] **Step 1: Write failing tests**
  - Server:
    - `tool_calls_listed_with_skills_used` — run with `fixtures/agent-responses/mcp/plugin_skill_tool_call.json` (existing: `skill_list`, `skill_load sample-plugin:hello`, `result_submit`) → items' `tool`s in order `["skill_list","skill_load","result_submit"]`, `source` `skill`,`skill`,`core`; `skillsUsed == ["sample-plugin:hello"]`.
    - `tool_calls_plugin_name_joined` — insert a `run_tool_calls` row with `source='plugin'`, `plugin_id=<sample-plugin id>` → `pluginName == "sample-plugin"`.
    - `tool_calls_unknown_run_404` and `tool_calls_requires_auth` (401 without session).
  - Web:
    - `RunToolsAndSkills lists calls and skills` — renders tool, source badge, status, duration (`12 ms`), plugin name for plugin rows, error text for errors, and a "Skills used" list.
    - `RunToolsAndSkills empty state` — "No tool calls recorded."
    - `TicketRunsTab details have tabs` — expanding a run shows tabs "Tools & Skills" (selected) and "Knowledge Used"; switching shows `KnowledgeUsed`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_mcp -- tool_calls_` and `cd web && yarn test src/features/runs src/features/tickets/TicketRunsTab` → FAIL.
- [ ] **Step 3: Implement** service, route, schemas, hook, component, and tabs (query only when the tab is visible).
- [ ] **Step 4: Run** the same commands → PASS.
- [ ] **Step 5: Commit** `feat(runs): tool-call log API and Tools & Skills tab`

### Task 8: Gateway tool names in live consoles

**Files:**
- Modify: `connectors/src/lib.rs`, `server/src/providers/claude_console.rs`, `server/src/providers/cursor_console.rs`, `server/src/providers/codex_console.rs`, the adapters that construct those publishers (`claude_code.rs`, `cursor.rs`, `codex.rs`), `web/src/opencode-session/parts/ToolPart.tsx`
- Create: `web/src/opencode-session/tools/gatewayTool.ts`, `web/src/opencode-session/tools/GatewayTool.tsx`
- Test: unit tests in `connectors/src/lib.rs` and each console file; `web/src/opencode-session/tools/gatewayTool.test.ts`, `web/src/opencode-session/parts/ToolPart.test.tsx` (create if absent)

**Interfaces:**
- Produces:
  - `coppice_connectors::GatewayTool { pub plugin: Option<String>, pub tool: String }`; `pub fn gateway_tool(style: ToolNameStyle, server: &str, name: &str) -> Option<GatewayTool>`; `pub fn gateway_tool_from_fields(server: &str, field_server: &str, tool: &str) -> Option<GatewayTool>`; `impl GatewayTool { pub fn title(&self, server: &str) -> String }` → `"coppice · ticket_get"` / `"github · create_issue"` (U+00B7 with spaces).
  - Publishers take `ToolNameStyle` (e.g. `ClaudeConsolePublisher::new(style)`), passed from the adapter's descriptor (`coppice_connectors::get(<id>).mcp_tool_names`); the server name argument is `protocol::SERVER_NAME`.
  - Web: `parseGatewayTool(name: string): { plugin: string | null; tool: string } | null` (prefix `coppice_`, split rest on first `__`).

- [ ] **Step 1: Write failing tests**
  - Connectors:
    - `gateway_tool_per_style` — `McpDoubleUnderscore`: `mcp__coppice__ticket_get` → `{None,"ticket_get"}`, `mcp__coppice__github__create_issue` → `{Some("github"),"create_issue"}`, `mcp__other__x` → `None`; `Dash`: `coppice-ticket_get` → core; `Underscore`: `coppice_github__create_issue` → plugin; `None` style → always `None`; `Bash` → `None` for every style.
    - `gateway_tool_from_fields_codex` — `("coppice","coppice","github__create_issue")` → plugin; field server `"other"` → `None`.
    - `title_format` — `"coppice · ticket_get"`, `"github · create_issue"`.
  - Server consoles (feed one tool-start line each, assert the published `title` and `variant == "action"`):
    - `claude_console_titles_gateway_tool` — tool `mcp__coppice__github__create_issue` → `github · create_issue`; `Bash` title unchanged.
    - `cursor_console_titles_gateway_tool` — `mcpToolCall` with `args.name == "coppice-ticket_get"` → `coppice · ticket_get`.
    - `codex_console_titles_gateway_tool` — `mcp_tool_call` server `coppice`, tool `github__create_issue` → `github · create_issue`; server `other` keeps `MCP other.tool`.
  - Web:
    - `parseGatewayTool` cases: `coppice_ticket_get` → `{plugin:null, tool:"ticket_get"}`; `coppice_github__create_issue` → plugin; `bash` → `null`.
    - `ToolPart renders gateway tool title` — part `tool: "coppice_github__create_issue"` renders `github · create_issue`; `tool: "bash"` still uses the Bash renderer.
- [ ] **Step 2: Run** `cargo test -p coppice-connectors`, `cargo test -p coppice-server --features embedded-test-db --lib -- providers::claude_console providers::cursor_console providers::codex_console`, `cd web && yarn test src/opencode-session` → FAIL.
- [ ] **Step 3: Implement** the helpers, publisher constructors, and `GatewayTool.tsx` (title + compact input summary, same layout as `UnknownTool`).
- [ ] **Step 4: Run** the same commands → PASS; `--test integration_cli_adapters` → PASS.
- [ ] **Step 5: Commit** `feat(consoles): render gateway and plugin tool names by ToolNameStyle`

### Task 9: Smoke, m03 fix, docs, final verification

**Files:**
- Create: `fixtures/plugins/m10-smoke/.claude-plugin/plugin.json` (`{"name":"m10-smoke","version":"0.1.0","description":"M10 smoke plugin"}`), `fixtures/plugins/m10-smoke/skills/greet/SKILL.md` (frontmatter `name: greet`, `description: Greets politely`), `fixtures/plugins/m10-smoke/.mcp.json` (`{"mcpServers":{"echo":{"command":"node","args":["${CLAUDE_PLUGIN_ROOT}/bin/echo-mcp.mjs"],"env":{"GREETING":"${GREETING}"}}}}`), `fixtures/plugins/m10-smoke/bin/echo-mcp.mjs` (newline-delimited JSON-RPC; one tool `echo`, `readOnlyHint: true`, returns `` `${process.env.GREETING}: ${args.text}` ``), `fixtures/agent-responses/mcp/m10_smoke.json` (`toolCalls`: `ticket_get {}`, `skill_load {"name":"m10-smoke:greet"}`, `m10-smoke__echo {"text":"hi"}`, `result_submit` done), `e2e/smoke/m10-plugins.mjs`
- Modify: `Makefile` (`.PHONY`, `e2e-smoke-m10`, `e2e-smoke-m03` env), `docs/architecture.md`, `docs/providers/README.md`, `AGENTS.md`, `docs/milestones/M10-plugins.md` (acceptance boxes this part completes)

- [ ] **Step 1: Write the smoke script** `e2e/smoke/m10-plugins.mjs` (helpers copied from `m09-chat.mjs`): login; `POST /api/plugin-dirs {path:"/app/fixtures/plugins/m10-smoke"}` (reuse the dir if it already exists); rescan; find plugin `m10-smoke`; `PUT settings {GREETING:"hello"}`; `POST test` → `echo` server `ok` with tool `m10-smoke__echo`; enable; create an agent and assign the plugin; create a board/ticket with a unique timestamped name; start the run; poll until finished `done`; `GET /api/agent-runs/:id/tool-calls` → tools include `ticket_get`, `skill_load`, `m10-smoke__echo` (source `plugin`), `result_submit`, all `ok`; `skillsUsed` includes `m10-smoke:greet`. Exit non-zero with a clear message on any mismatch.
- [ ] **Step 2: Makefile** — `e2e-smoke-m10`: `$(MAKE) compose-up`, `$(SMOKE_REPO_SETUP_IF_MISSING)` if the run needs a repo (model on `e2e-smoke-m06-knowledge`), `MOCK_AGENT_RESPONSE=mcp/m10_smoke WORKFLOW_AUTO_START_RUNS=false $(COMPOSE) up -d --force-recreate --no-deps server`, `node e2e/smoke/m10-plugins.mjs`. `e2e-smoke-m03`: recreate the server with `MOCK_AGENT_RESPONSE=done WORKFLOW_AUTO_START_RUNS=false` the same way before running its script.
- [ ] **Step 3: Docs** — `docs/architecture.md`: plugin MCP proxy section (pool, transports, source, settings, placeholder rules, list timeout) and an "Adding an MCP transport" checklist (implement `McpTransport`, register in `Transports::builtin()`, map the kind in `resolve`/`McpServerTransport` if new, test with a loopback stub). `docs/providers/README.md`: console tool titles come from `ToolNameStyle`. `AGENTS.md`: status line and smoke list (`make e2e-smoke-m10`). M10 milestone: tick completed items.
- [ ] **Step 4: Final verification** — `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test` → pass. `make compose-up`, `make e2e-smoke-m10`, `make e2e-smoke-m03`, `make e2e-smoke-m09` → pass. (m03 hits a fixed board name on a database that already ran it; if so, record it — do not run `docker compose down -v`.)
- [ ] **Step 5: Commit** `test(m10): plugin MCP smoke; fix m03 smoke env; docs`
- [ ] **Step 6:** after the final whole-branch review is clean, `make clean`.
