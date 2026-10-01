# Connector and Tool-Source Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Coppice explicit extension points — connector descriptors, MCP wiring renderers, a shared CLI runner, a gateway `ToolSource` registry, and plugin capability parsers — with no behavior change, so Part 2b and future connectors/plugins plug in instead of editing scattered match statements.

**Architecture:** A new `connectors` workspace crate holds static connector descriptors used by server, CLI, and (through `/api/connectors`) the web. The server builds connectors from one factory list, renders MCP config from one `McpServerSpec`, and runs CLI adapters through one `run_cli` loop. The MCP gateway dispatches through a `ToolRegistry` of `ToolSource`s; plugin manifests are assembled from one parser per capability.

**Tech Stack:** Rust (Axum, SQLx, tokio, serde, async-trait), React + TanStack Query + Vitest, embedded Postgres tests.

**Spec:** [docs/superpowers/specs/2026-10-01-connector-and-tool-source-foundations-design.md](../specs/2026-10-01-connector-and-tool-source-foundations-design.md) (amends [M10 Plugins Design](../specs/2026-09-29-m10-plugins-design.md)).

## Global Constraints

- **No behavior change.** Same `[agent.connectors.<id>]` config keys and options, same CLI arguments and env per connector, same per-run MCP file bytes, same tool names and profile matrix, same live console events, same user-visible error strings, same plugins API response shape.
- Descriptor baseline values are exactly the spec's table (ids `mock`, `claude-code`, `cursor`, `codex`, `kilo-code`, `opencode`; capabilities `read_only_tools`, `chat_resume`, `session_events`, `run_resume`, `run_server`).
- MCP server name `"coppice"` and token env `"COPPICE_MCP_TOKEN"` appear only in `server/src/mcp/wiring.rs` (reusing `protocol::SERVER_NAME`) and `mcp/grant.rs`.
- Run tokens reach CLIs only via process env or per-run files under `<artifacts_dir>/runs/<run_id>/` (or Cursor's existing state dir) — never the worktree, a repo checkout, or global CLI config.
- `run_tool_calls.source` ∈ `core | skill | plugin`; `plugin_id` set only for plugin-owned tools.
- Server owns state; handlers stay thin. Tests use `state.db`, never `db::shared_test_pool()`. Agent tests use `MockProvider`.
- Iterate with `make test-unit`, targeted `cargo test -p coppice-server --features embedded-test-db <filter>`, `cargo test -p coppice-cli`, `cd web && yarn test <path>`. `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test` only at the end (Task 9), then `make clean`.
- Do not run repo-wide `cargo fmt`; format only files you touch. Pre-existing `yarn lint` errors are not yours to fix.

## Review Focus

1. **Cancel or stop mid-run** — after the runner migration, a cancelled CLI run still kills the child and returns `ProviderError::Cancelled`; a timed-out run kills the child and keeps each adapter's exact timeout message. (Task 5: `run_cli_cancel_kills_child`, `run_cli_timeout_kills_child_and_returns_tail`.)
2. **Cursor quirks survive** — a result event followed by a non-zero exit is still accepted; error messages still carry the stderr tail suffix and `` `{command}` `` prefix. (Task 6: `cursor_accepts_result_despite_nonzero_exit`, `cursor_timeout_message_has_stderr_suffix`.)
3. **Agent whose connector is not configured** — health still reports `missing_config` with "Connector 'x' is not configured on this server"; the drawer falls back to the plain console. (Task 2: `health_unconfigured_connector_message_unchanged`; Task 3: `unknown connector falls back to plain console`.)
4. **Chat profile tool list** — after the registry, `human_chat`/`conversation` list exactly today's tools (`result_submit` kept, `comment_post` absent) and a non-read-only tool from any source is dropped. (Task 7: `chat_profiles_list_unchanged`, `chat_filter_drops_non_read_only_but_keeps_result_submit`.)
5. **Stored manifests from Part 2a** — a manifest JSON with `mcpServers[].kind` and string `unsupported` still loads (skills served) before the startup rescan, and `GET /api/plugins` returns the same shape. (Task 8: `old_manifest_json_deserializes`, `plugins_api_shape_unchanged`.)

---

### Task 1: `connectors` crate; CLI migrated

**Files:**
- Create: `connectors/Cargo.toml` (package `coppice-connectors`, deps: none beyond `serde` with `derive`), `connectors/src/lib.rs`
- Modify: `Cargo.toml` (workspace members), `cli/Cargo.toml`, `cli/src/commands/connector/registry.rs` (delete `ConnectorId`, `ConnectorMeta`, `CONNECTORS`; keep any CLI-only helpers), every `cli/src/commands/connector/*.rs` that used them, `deploy/` Dockerfiles if they copy workspace members explicitly

**Interfaces:**
- Produces (`coppice_connectors`): `ConnectorDescriptor`, `InstallInfo { auth_hint, auth_paths, auth_env }`, `McpWiring`, `ToolNameStyle`, `ConsoleKind`, `Capabilities` — fields and variants exactly as in the spec; `pub fn all() -> &'static [ConnectorDescriptor]`; `pub fn get(id: &str) -> Option<&'static ConnectorDescriptor>`; id constants `MOCK`, `CLAUDE_CODE`, `CURSOR`, `CODEX`, `KILO_CODE`, `OPENCODE`. `ConsoleKind` and `Capabilities` derive `Serialize` (camelCase: `openCodeSession`, `structured`, `plain`; `readOnlyTools`, …).

- [ ] **Step 1: Write failing tests** in `connectors/src/lib.rs`:
  - `ids_are_unique_and_match_constants` — `all()` ids are unique and equal the six constants.
  - `baseline_capabilities` — for each row of the spec table assert `mcp_wiring`, `mcp_tool_names`, `console`, and all five caps (e.g. `get("cursor")` → `CursorHome`, `Dash`, `Structured`, read_only ✓, chat_resume ✓, session_events ✓, run_resume ✓, run_server —; `get("opencode")` → run_server ✓, run_resume —; `get("kilo-code")` → chat_resume —).
  - `install_info_moved_verbatim` — binary/auth values equal the deleted CLI table (e.g. cursor `binary == "agent"`, `auth_paths == [".config/cursor/auth.json", ".cursor/auth.json"]`; claude-code `auth_env == ["ANTHROPIC_API_KEY"]`; opencode `auth_paths == [".local/share/opencode"]`).
  - `get_unknown_is_none`.
- [ ] **Step 2: Run** `cargo test -p coppice-connectors` → FAIL (crate/items missing).
- [ ] **Step 3: Implement** the crate; then switch the CLI to it (parse via `get`, iterate `all()`), deleting the old table.
- [ ] **Step 4: Run** `cargo test -p coppice-connectors && cargo test -p coppice-cli` → PASS; `cargo run -p coppice-cli -- connector list` output identical to before (paste both in the report).
- [ ] **Step 5: Commit** `feat(connectors): shared connector descriptor crate; CLI uses it`

### Task 2: Server uses descriptors; factory registry; `ModelCatalog`; `/api/connectors` caps

**Files:**
- Create: `server/src/providers/models.rs` (`ModelCatalog`, `ModelInfo`, per-connector catalogs wrapping `opencode_models`, `codex_models`, `kilo_models`, `cursor_models`, and `known_claude_code_models` moved from `api/connectors.rs`)
- Modify: `server/Cargo.toml`, `server/src/providers/registry.rs`, `server/src/providers/mod.rs`, `server/src/api/connectors.rs`, `server/src/services/agent_health.rs`, `server/src/workers/job_worker.rs` (session/resume checks), `server/src/api/ws/live.rs`, `server/src/workers/run_watchdog.rs`, `server/src/services/agent_service.rs` (`"mock"` → `coppice_connectors::MOCK`), `server/src/lib.rs` (test helper)
- Test: `server/src/providers/registry.rs` (unit), `server/tests/integration_agents.rs` or the existing connectors API test file

**Interfaces:**
- Consumes: Task 1 crate.
- Produces: `ConnectorFactory { id: &'static str, build: fn(&AppConfig, &FactoryDeps) -> Option<BuiltConnector> }`, `FactoryDeps { opencode_runs: Arc<OpenCodeRunServers> }`, `BuiltConnector { provider: Arc<dyn AgentProvider>, models: Arc<dyn ModelCatalog> }`, `pub static FACTORIES: &[ConnectorFactory]`; `ModelCatalog { fn model_providers(&self) -> &[String]; async fn list_models(&self, model_provider: &str) -> anyhow::Result<Vec<ModelInfo>> }`, `ModelInfo { id: String, name: String }`; `ConnectorRegistry::{has, get, configured_ids, models(&self, id) -> Option<Arc<dyn ModelCatalog>>, has_model_provider}`; `providers::descriptor(id) -> Option<&'static ConnectorDescriptor>` convenience. `connector_enforces_read_only` / `connector_supports_chat_resume` read `caps`. `GET /api/connectors` items: `{ id, displayName, console, caps: { readOnlyTools, chatResume } }`.

- [ ] **Step 1: Write failing tests:**
  - `factories_and_descriptors_match` — every `FACTORIES` id has a descriptor and every descriptor id has a factory.
  - `opencode_enabled_when_default_connector` — config with opencode disabled but `default_connector = "opencode"` still registers it (today's rule).
  - `no_connector_literals_outside_providers` — reads each `server/src/**/*.rs` up to its first `#[cfg(test)]`, skipping `providers/` and `sessions/opencode*`, and fails listing `file:line` for any string literal `"mock" | "claude-code" | "cursor" | "codex" | "kilo-code" | "opencode"`.
  - `health_unconfigured_connector_message_unchanged` — `"Connector 'cursor' is not configured on this server"`; and model provider not configured → `"Model provider 'x' is not configured on this server"`.
  - Integration: `list_connectors_returns_console_and_caps` — mock entry is `{ id: "mock", displayName: <descriptor>, console: "plain", caps: { readOnlyTools: true, chatResume: true } }`; existing model-provider/model list tests still pass.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db factories_and_descriptors no_connector_literals health_unconfigured list_connectors` → FAIL.
- [ ] **Step 3: Implement.** Registry iterates `FACTORIES`; health uses the generic rule; `list_models` calls `registry.models(id)` and maps errors to 502 `"{id} models: {err}"` (cursor's catalog returns empty for providers other than `"cursor"`, mock returns empty). `job_worker` uses `caps.session_events` / `caps.run_resume`; `live.rs` uses `console`; watchdog uses `caps.run_server`.
- [ ] **Step 4: Run** the Step 2 filter plus `cargo test -p coppice-server --features embedded-test-db connectors agent_health job_worker live_console run_watchdog` → PASS.
- [ ] **Step 5: Commit** `refactor(connectors): server reads connector descriptors; factory registry and ModelCatalog`

### Task 3: Web picks the live view from `console`

**Files:**
- Modify: `web/src/features/agents/useAgents.ts` (connectors query parses the new fields), `web/src/features/tickets/TicketDrawer.tsx`
- Create or extend: `web/src/lib/schemas/connector.ts` (if no schema exists), `web/src/features/tickets/TicketDrawer.test.tsx` (extend if present)

**Interfaces:**
- Consumes: Task 2 `/api/connectors` shape.
- Produces: `connectorSchema = { id, displayName, console: 'openCodeSession' | 'structured' | 'plain', caps: { readOnlyTools, chatResume } }`; `useConnectors()` returns parsed items; `liveViewFor(console: ConnectorConsole | undefined)` in `TicketDrawer.tsx` (or a sibling module) → `LiveSession | ClaudeLiveConsole | LiveConsole`.

- [ ] **Step 1: Write failing tests:** `uses OpenCode session view for openCodeSession`, `uses structured console for structured`, `unknown connector falls back to plain console` (run connector absent from the connectors list → `LiveConsole`). Existing callers of the connectors query (agent form connector select) keep working with the richer items.
- [ ] **Step 2: Run** `cd web && yarn test src/features/tickets src/features/agents` → FAIL.
- [ ] **Step 3: Implement**; remove the `connector === '…'` comparisons from `TicketDrawer.tsx`.
- [ ] **Step 4: Run** the same command → PASS; `rg -n "=== '(opencode|claude-code|codex|kilo-code|cursor)'" web/src --glob '!*.test.*'` returns nothing.
- [ ] **Step 5: Commit** `refactor(web): choose live console from connector descriptor`

### Task 4: `McpServerSpec` renderers; adapters switched

**Files:**
- Create: `server/src/mcp/wiring.rs`
- Modify: `server/src/mcp/mod.rs`, `server/src/providers/claude_code.rs` (`claude_mcp_args`), `server/src/providers/cursor.rs` (`cursor_mcp_setup`, `McpFile`, `CliConfigFile`), `server/src/providers/codex.rs` (`codex_mcp_args`), `server/src/providers/kilo_code.rs` (`kilo_mcp_setup`), `server/src/sessions/opencode_run_server.rs` (config body)

**Interfaces:**
- Produces: `McpServerSpec { name: &'static str, url: String, token_env: &'static str }` with `from_access(&McpAccess) -> Self`, `claude_json() -> String`, `cursor_mcp_json() -> String`, `cursor_cli_config() -> String`, `opencode_json() -> String`, `kilo_json() -> String`, `codex_args() -> Vec<String>`. `McpAccess::env()` stays the env source.

- [ ] **Step 1: Write snapshot tests first, against today's output.** For URL `http://127.0.0.1:5000/mcp`, call the **existing** functions and record their exact output as string constants: `claude_json_matches_baseline`, `cursor_mcp_json_matches_baseline` (today: `{"mcpServers":{"coppice":{"url":"http://127.0.0.1:5000/mcp","headers":{"Authorization":"Bearer ${env:COPPICE_MCP_TOKEN}"}}}}`), `cursor_cli_config_matches_baseline`, `opencode_json_matches_baseline` (today's `opencode_run_server` body), `kilo_json_matches_baseline`, `codex_args_match_baseline` (today: `-c`, `mcp_servers.coppice.url="http://127.0.0.1:5000/mcp"`, `-c`, `mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"`, in today's order). Point each test at `McpServerSpec`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db wiring::` → FAIL (type missing).
- [ ] **Step 3: Implement** the renderers, then make each adapter call them (file writes and flags otherwise unchanged).
- [ ] **Step 4: Run** `cargo test -p coppice-server --features embedded-test-db wiring:: claude_code cursor codex kilo opencode_run_server` → PASS (existing adapter tests unchanged); `rg -n '"COPPICE_MCP_TOKEN"|mcp_servers\.coppice|"coppice"' server/src/providers server/src/sessions` returns nothing.
- [ ] **Step 5: Commit** `refactor(mcp): one McpServerSpec rendered per connector wiring style`

### Task 5: `run_cli` + fake CLI; claude-code and codex migrated

**Files:**
- Create: `server/src/providers/cli_runner.rs`, `server/tests/support/fake_cli.rs` (bin `fake-cli`, `required-features = ["embedded-test-db"]`, registered in `server/Cargo.toml`)
- Modify: `server/src/providers/mod.rs`, `server/src/providers/claude_code.rs`, `server/src/providers/codex.rs`
- Test: `server/tests/integration_cli_runner.rs`

**Interfaces:**
- Produces: `CliInvocation { program: String, args: Vec<String>, env: Vec<(String, String)>, cwd: PathBuf, timeout: Duration }`; `trait LineHandler: Send { fn on_json(&mut self, value: &Value) -> LineStep }`; `LineStep { session_id: Option<String>, stop: bool }`; `RunIo { cancel_rx: Option<watch::Receiver<bool>>, session_created_tx: Option<watch::Sender<String>>, stderr_target: &'static str }`; `async fn run_cli(inv, handler: &mut dyn LineHandler, io: RunIo) -> Result<CliExit, CliError>`; `CliExit { status: ExitStatus, stderr_tail: Vec<String> }` (first 40 stderr lines); `CliError::{Spawn(io::Error), Cancelled, TimedOut { stderr_tail: Vec<String> }, Io(io::Error)}`. The duplicated `is_cancelled` / `wait_cancel` helpers move into `cli_runner.rs`.
- `fake-cli` behavior from env: `FAKE_CLI_LINES` (newline-separated stdout lines), `FAKE_CLI_STDERR`, `FAKE_CLI_EXIT` (code), `FAKE_CLI_SLEEP_MS` (sleep after printing).

- [ ] **Step 1: Write failing tests** (`integration_cli_runner.rs`, using `env!("CARGO_BIN_EXE_fake-cli")`):
  - `run_cli_reads_json_lines_until_stop` — non-JSON lines skipped; handler sees JSON lines in order; returns after `stop`.
  - `run_cli_forwards_first_session_id_once` — two lines with session ids → `session_created_tx` receives only the first.
  - `run_cli_cancel_kills_child` — sleep 30 s, set cancel → `CliError::Cancelled` within 2 s and the child pid is gone.
  - `run_cli_timeout_kills_child_and_returns_tail` — timeout 300 ms, stderr `boom` → `TimedOut { stderr_tail: ["boom"] }`.
  - `run_cli_returns_status_and_tail` — exit 3 with 50 stderr lines → `status.code() == Some(3)`, `stderr_tail.len() == 40`.
  - `run_cli_spawn_failure` — missing program → `CliError::Spawn`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_cli_runner` → FAIL.
- [ ] **Step 3: Implement** `run_cli`; then migrate claude-code and codex: each builds a `CliInvocation`, a handler wrapping its console publisher and text accumulation (claude: stop on `type == "result"`, taking `result` as final text), and maps `CliError`/`CliExit` to today's exact `ProviderError` messages (`"claude-code run timed out after {n}s"`, `"claude-code exited with status {status}"`, codex equivalents).
- [ ] **Step 4: Run** `cargo test -p coppice-server --features embedded-test-db --test integration_cli_runner` and `cargo test -p coppice-server --features embedded-test-db claude_code codex` → PASS.
- [ ] **Step 5: Commit** `refactor(providers): shared CLI runner; claude-code and codex use it`

### Task 6: cursor and kilo-code on `run_cli`

**Files:**
- Modify: `server/src/providers/cursor.rs`, `server/src/providers/kilo_code.rs`
- Test: unit tests in those files (with `fake-cli` via the adapter's configurable `command`)

**Interfaces:**
- Consumes: Task 5 `run_cli`, `LineHandler`, `CliExit`, `CliError`.

- [ ] **Step 1: Write failing tests** driving each adapter with `command = <fake-cli path>`:
  - `cursor_accepts_result_despite_nonzero_exit` — result event then exit 1 → `Ok` result parsed from the event's `result`.
  - `cursor_result_error_message` — result event with error → `` "`{command}` result error: {msg}{stderr suffix}" ``.
  - `cursor_timeout_message_has_stderr_suffix` — `` "`{command}` timed out after {n}s" `` followed by today's `format_stderr_suffix` text.
  - `cursor_cancel_returns_cancelled`.
  - `kilo_stops_on_session_idle` — `{"type":"session.idle"}` ends the stream; `kilo_timeout_message` → `"kilo-code run timed out after {n}s"`.
  Write these against the current adapters first; they must pass before migration (they pin behavior), then stay green after.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db cursor kilo` → PASS on current code (baseline recorded in report).
- [ ] **Step 3: Migrate** both adapters to `run_cli` (Cursor keeps its per-run `HOME` setup, `tracing::info!` spawn log, and `` "failed to spawn `{command}` (cwd …)" `` mapping for `CliError::Spawn`).
- [ ] **Step 4: Run** the same command → PASS; `rg -n "fn wait_cancel|fn is_cancelled" server/src/providers` shows only `cli_runner.rs` (opencode/mock may keep their own if they have them).
- [ ] **Step 5: Commit** `refactor(providers): cursor and kilo-code use the shared CLI runner`

### Task 7: `ToolSource` and `ToolRegistry`

**Files:**
- Create: `server/src/mcp/source.rs`, `server/src/mcp/registry.rs`
- Modify: `server/src/mcp/protocol.rs` (async `list`, `ToolResult` content blocks), `server/src/mcp/host.rs`, `server/src/mcp/catalog.rs` (drop skill tools from `CoreTool`), `server/src/mcp/tools/mod.rs`, `server/src/mcp/tools/skills.rs`, `server/src/mcp/server.rs`, `server/src/lib.rs` (`AppState.tools: Arc<ToolRegistry>`), test bootstrap in `server/tests/common/mod.rs` if it builds `AppState`
- Test: unit tests in `registry.rs`; `server/tests/integration_mcp.rs`

**Interfaces:**
- Produces: `SourceKind::{Core, Skill, Plugin}` with `as_str()` → `"core" | "skill" | "plugin"`; `SourcedTool { def: ToolDefinition, source: SourceKind, plugin_id: Option<Uuid>, key: String }`; `ToolContent::{Text(String), Image { data: String, mime_type: String }}`; `ToolResult { content: Vec<ToolContent>, is_error: bool }`; `#[async_trait] trait ToolSource { fn kind(&self) -> SourceKind; async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool>; async fn call(&self, ctx: &ToolCtx<'_>, tool: &SourcedTool, args: Value) -> Result<ToolResult, ToolError>; }`; `CoreToolSource`, `SkillToolSource`; `ToolRegistry::new(sources: Vec<Arc<dyn ToolSource>>)`, `async fn tools_for(&self, scope) -> Vec<SourcedTool>`, `async fn call(&self, ctx, name, args) -> (CallStatus, ToolResult, Option<String>, Option<&SourcedTool info>)` (shape is the implementer's; logging must get `source` and `plugin_id`). `ToolHost::list` is `async`; `ToolHost::call` returns `ToolResult`. Part 2b registers `PluginMcpSource` via `ToolRegistry::new` without touching the router.

- [ ] **Step 1: Write failing tests:**
  - `merge_keeps_source_order_and_first_duplicate` — two fake sources both exposing `x`; listed once, from the first; a warning is logged once.
  - `chat_profiles_list_unchanged` — for every `ContextProfile`, `tools_for` names equal today's lists (`full`/`human_agent`: board_agents, ticket_get, ticket_comments, ticket_runs, ticket_search, knowledge_search, comment_post, result_submit, skill_list, skill_load; `human_chat`/`conversation`: same minus comment_post; `knowledge_compaction`: ticket_get, ticket_comments, ticket_runs, knowledge_search, skill_list, skill_load, result_submit).
  - `chat_filter_drops_non_read_only_but_keeps_result_submit` — fake source tool with `read_only = false` is listed for `full`, absent for `human_chat`/`conversation`.
  - `denied_text_unchanged` — `denied: tool "nope" is not available for this run`, logged `source = core`, status `denied`.
  - `fake_source_call_logs_source_and_plugin_id` — a fake `Plugin` source tool with `plugin_id = P` → `run_tool_calls` row with `source = 'plugin'`, `plugin_id = P`.
  - `image_content_rendered` — protocol renders `{type:"image",data,mimeType}`.
  - Existing `integration_mcp.rs` tests unchanged and passing; add an assertion that a `skill_load` call row has `source = 'skill'`.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db mcp::` and `--test integration_mcp` → FAIL.
- [ ] **Step 3: Implement.** Timeout and output cap (with the existing truncation suffix, applied to text blocks) live in the registry; core handlers keep returning JSON wrapped as one text block.
- [ ] **Step 4: Run** the same commands plus `--test integration_chat --test integration_knowledge_compaction --test integration_plugins` → PASS.
- [ ] **Step 5: Commit** `refactor(mcp): ToolSource registry; core and skill sources; source and plugin_id logged`

### Task 8: Plugin capability parsers; full MCP server entries

**Files:**
- Create: `server/src/plugins/capability.rs`
- Modify: `server/src/plugins/manifest.rs`, `server/src/plugins/mod.rs`, `server/src/plugins/skills.rs` (test helpers constructing manifests), `server/src/api/plugins.rs` (response mapping), fixtures under `fixtures/plugins/` (add an inline-`mcpServers` fixture `fixtures/plugins/inline-mcp/`)
- Test: unit tests in `capability.rs` / `manifest.rs`; `server/tests/integration_plugins.rs`

**Interfaces:**
- Produces: `CapabilityOutcome<T>::{Absent, Supported(T), Unsupported(String)}`; `trait CapabilityParser { type Output; const KEY: &'static str; fn parse(root: &Path, plugin_json: Option<&Value>, layout: PluginLayout) -> CapabilityOutcome<Self::Output>; }`; `SkillsCapability`, `McpServersCapability`, `CommandsCapability`, `AgentsCapability`, `HooksCapability`; `McpServerEntry { name: String, transport: McpServerTransport, error: Option<String> }`; `McpServerTransport::{Stdio { command, args: Vec<String>, env: BTreeMap<String, String> }, Http { url, headers: BTreeMap<String, String> }, Unsupported { kind: String }}` (serde tag `type`, camelCase); `UnsupportedPart { key: String, reason: String }`; `McpServerEntry::api_kind(&self) -> &str` (`"stdio"`, `"http"`, or the unsupported kind). `PluginManifest { …, skills, mcp_servers: Vec<McpServerEntry>, unsupported: Vec<UnsupportedPart> }`. Part 2b reads `McpServerTransport` to start servers.

- [ ] **Step 1: Write failing tests:**
  - `mcp_stdio_entry_keeps_full_spec` — `.mcp.json` `{"mcpServers":{"fs":{"command":"${CLAUDE_PLUGIN_ROOT}/bin/fs","args":["--root","${ROOT}"],"env":{"TOKEN":"${API_TOKEN}"}}}}` → `Stdio` with placeholders preserved verbatim.
  - `mcp_http_entry_keeps_url_and_headers` — `{"type":"http","url":"https://x/mcp","headers":{"Authorization":"Bearer ${KEY}"}}`.
  - `mcp_sse_and_unknown_are_unsupported` — `type: "sse"` → `Unsupported { kind: "sse" }`; neither command nor url → `Unsupported { kind: "unknown" }`.
  - `inline_plugin_json_mcp_servers` — `fixtures/plugins/inline-mcp` with `plugin.json` `"mcpServers": { … }` and no `.mcp.json` parses the same entry.
  - `malformed_mcp_json_marks_plugin_invalid` — unchanged ruling.
  - `unsupported_dirs_reported` — `commands/`, `agents/`, `hooks/` → `UnsupportedPart { key, reason: "not supported yet" }`.
  - `old_manifest_json_deserializes` — a Part 2a manifest JSON (`"mcpServers":[{"name":"fs","kind":"stdio"}]`, `"unsupported":["commands"]`) deserializes; skills intact; `transport == Unsupported { kind: "unknown" }`; `unsupported[0].key == "commands"`.
  - Integration `plugins_api_shape_unchanged` — `GET /api/plugins` for `fixtures/plugins/sample-plugin` returns `mcpServers: [{ name, kind }]` and `unsupported: [String]`, with no `command`/`env`/`url`/`headers` keys anywhere in the response.
  - Existing Task 3 (Part 2a) manifest and discovery tests still pass.
- [ ] **Step 2: Run** `cargo test -p coppice-server --features embedded-test-db plugins::` and `--test integration_plugins` → FAIL.
- [ ] **Step 3: Implement** the parsers and assemble `PluginManifest` in `parse_plugin`; keep skill containment rules unchanged.
- [ ] **Step 4: Run** the same commands → PASS; `cd web && yarn test src/features/plugins` → PASS with no web changes.
- [ ] **Step 5: Commit** `refactor(plugins): capability parsers; MCP server entries keep full spec`

### Task 9: Docs and final verification

**Files:**
- Modify: `docs/architecture.md` (connector layer, tool sources, capabilities; "Adding a connector", "Adding a tool source", "Adding a plugin capability" checklists from the spec), `docs/providers/README.md` (descriptor table is the source of truth; link), `AGENTS.md` monorepo map (add `connectors/`)

- [ ] **Step 1: Write the docs.**
- [ ] **Step 2: Final verification:** `make test`, `cargo clippy --workspace -- -D warnings`, `make web-test` → all pass. Then `make compose-up`, `make e2e-smoke-m09` → pass. For `make e2e-smoke-m03`: it is known to fail when re-run on a database that already ran it (fixed board name) — run it only if the stack's database has not run it; otherwise record that in the report. Do **not** run `docker compose down -v`.
- [ ] **Step 3: Commit** `docs(connectors): extension checklists for connectors, tool sources, plugin capabilities`
- [ ] **Step 4:** after the final whole-branch review is clean, `make clean`.
