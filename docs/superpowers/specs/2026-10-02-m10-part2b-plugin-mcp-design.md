# M10 Part 2b — Plugin MCP Proxy and Observability Design

**Status:** Implemented (approved 2026-10-02; acceptance verified 2026-10-03)  
**Parent:** [M10 Plugins design](2026-09-29-m10-plugins-design.md) — "Plugin MCP proxy", steps 6–7  
**Builds on:** [Connector and tool-source foundations](2026-10-01-connector-and-tool-source-foundations-design.md) (merged)

## Goal

Proxy plugin MCP servers (stdio and streamable HTTP) through the Coppice gateway as a third tool source, with encrypted plugin settings, a Test button, a per-run tool-call log in the UI, and gateway tool names rendered the same way in every live console. Every extension point stays a one-place change: a new MCP transport is one `McpTransport` implementation; a new tool source is one `ToolSource`; a new connector's console only needs its descriptor's `ToolNameStyle`.

## Decisions (2026-10-02)

| Question | Decision |
|----------|----------|
| `${VAR}` resolution | Plugin setting `VAR` → server environment `VAR`, except names starting with `COPPICE_` and the exact names `DATABASE_URL`, `SECRETS_MASTER_KEY` → otherwise the server fails to start with `missing setting "VAR"`. `${VAR:-default}` uses `default` when both are absent. |
| stdio child environment | Cleared, then `PATH`, `HOME`, `LANG`, `TMPDIR` copied from the server process when set, then the entry's resolved `env`. Working directory: plugin root. |
| Tools & Skills location | Inside the expanded run row on the ticket's Runs tab, as tabs **Tools & Skills** / **Knowledge Used**. No new run page. |
| m03 smoke | The `e2e-smoke-m03` Makefile target sets `MOCK_AGENT_RESPONSE=done WORKFLOW_AUTO_START_RUNS=false` (pre-existing failure). |

## Plugin settings

- Table `plugin_settings (plugin_id → plugins ON DELETE CASCADE, key, secret_id → secrets ON DELETE CASCADE, updated_at, PRIMARY KEY (plugin_id, key))`.
- Values are stored with the M07 `SecretService` under the secret name `plugin-setting-<plugin_id>-<key>`.
- **Keys** are the `${VAR}` names used in the plugin's MCP entries (command, args, env values, url, header values), excluding `CLAUDE_PLUGIN_ROOT`. Unknown keys are rejected (`unknown setting "<key>"`).
- `PUT /api/plugins/:id/settings` (admin, CSRF) with `{ "values": { "<KEY>": "<value>" } }`; an empty string clears the key. Values are write-only: plugin responses carry `settings: [{ key, configured, source }]`, where `source` (`setting | env | default | missing`) is computed from presence only.

## Placeholder resolution

`plugins::placeholders` is pure. `${CLAUDE_PLUGIN_ROOT}` → plugin root path. `${NAME}` / `${NAME:-default}` with `NAME = [A-Za-z_][A-Za-z0-9_]*` resolve per the decision table. An unterminated `${` is kept verbatim. Resolution happens only when a server starts; stored manifests keep placeholders. Resolution errors name the key, never a value.

## Transports

`server/src/mcp/proxy/transport.rs` defines the seam; `rmcp` 3.5 is used only as a client inside implementations:

- `McpTransport { kind() -> &'static str; connect(spec, cwd) -> Box<dyn McpConnection> }`
- `McpConnection { list_tools(); call_tool(name, args); take_tools_changed() -> bool; is_closed(); close() }`
- `StdioTransport` (`kind = "stdio"`, child with the environment above, `kill_on_drop`, stderr lines to tracing target `coppice::plugin_mcp` at debug) and `HttpTransport` (`kind = "http"`, streamable HTTP with the resolved headers).
- `Transports` maps kind → transport; `Transports::builtin()` registers stdio and http. SSE or any later transport is one implementation plus one registration.
- Content mapping: text → `ToolContent::Text`, image → `ToolContent::Image`, anything else → text `[unsupported content: <type>]`.
- Error messages never include resolved env values, header values, or URLs with credentials.

## `McpServerPool`

One shared instance per `(plugin_id, server name)` across runs.

- **Lazy start** on first `tools`/`call`/`test`. Start runs in a spawned task bounded by `mcp_start_timeout_secs`, so a caller's timeout does not cancel it.
- **Fingerprint:** hash of the resolved transport. A request whose fingerprint differs from the running instance (settings or manifest changed) restarts it.
- **Health:** `stopped | starting | ready | backoff | unhealthy`. A failed start or a closed connection counts as a failure and schedules a restart with exponential backoff (1 s doubling, max 60 s). Three consecutive failures → `unhealthy` (tools hidden, calls fail) until `test` or a fingerprint change. A placeholder error is `unhealthy` immediately. A successful start resets the count.
- **Tool cache:** filled on start; refreshed on `notifications/tools/list_changed` and on restart.
- **Idle shutdown:** a reaper stops instances unused for `mcp_idle_shutdown_secs`.
- **Hooks:** disabling a plugin and git-updating it call `stop_plugin`. Settings and manifest changes rely on the fingerprint.
- Concurrent calls share one connection (rmcp multiplexes JSON-RPC ids).

Config `[plugins]`: `mcp_start_timeout_secs = 20`, `mcp_list_timeout_secs = 10`, `mcp_idle_shutdown_secs = 600`.

## `PluginMcpSource`

- Third `ToolSource`, registered after core and skill sources. Holds `Arc<McpServerPool>` and an `Arc<dyn PluginServerCatalog>`; the DB catalog loads enabled `ok` plugins in the token's `plugin_ids` snapshot with decrypted settings.
- Lists nothing for `knowledge_compaction`. Skips unsupported, unhealthy, and failing servers (logged).
- Exposed name `<plugin>__<tool>`: each part sanitized to `[A-Za-z0-9_-]` (others → `_`); if longer than 50 chars, the first 41 chars + `_` + the first 8 hex chars of SHA-256 of the unsanitized `<plugin>__<tool>`. 50 keeps `mcp__coppice__<name>` within 64.
- Descriptions and `readOnlyHint` pass through, so the registry's chat filter applies unchanged. Logged with `source = plugin` and `plugin_id`.
- A call to a server that is down returns tool error `plugin "<name>" unavailable` (with `: missing setting "X"` for placeholder errors).
- **Registry:** `ToolRegistry` bounds each source's `list` by a list timeout (default 10 s, from `mcp_list_timeout_secs` in production). A source that times out contributes no tools for that request and logs a warning; core tools are unaffected.

## Test button and warnings

- `POST /api/plugins/:id/test` (admin, CSRF), allowed while disabled: restarts every MCP server of the plugin and returns `{ servers: [{ name, kind, status: ok | error | unsupported, error?, tools: [{ name, exposedName, description, readOnly }] }] }`. A successful test clears `unhealthy`.
- Plugin responses add `mcpServers[].health` (`stopped | starting | ready | backoff | unhealthy`). No command, args, env, URL, header, or setting value is ever returned.
- Enabling a plugin with any stdio server asks for confirmation: "This plugin starts local MCP servers that run with the Coppice server's privileges until sandboxing lands (M11). Enable anyway?"

## Observability

- `GET /api/agent-runs/:id/tool-calls` → `{ items: [{ id, tool, source, pluginId, pluginName, status, error, durationMs, argsSummary, createdAt }], skillsUsed: [string] }`, oldest first. `skillsUsed` is the distinct `name` argument of successful `skill_load` calls, in first-use order.
- The run row's details show tabs **Tools & Skills** (default) and **Knowledge Used**.
- `coppice_connectors::gateway_tool(style, server, name) -> Option<GatewayTool { plugin: Option<String>, tool: String }>` strips the style's gateway prefix (`mcp__<server>__`, `<server>-`, `<server>_`) and splits the rest on the first `__` into plugin and tool. `gateway_tool_from_fields(server, field_server, tool)` covers `ServerToolFields` (codex). Claude, Cursor, and Codex console publishers take their descriptor's style and title gateway calls `coppice · <tool>` or `<plugin> · <tool>`. The OpenCode web tool part applies the same rule for `Underscore`. Kilo emits no tool events and is unchanged.

## Smoke and docs

- `make e2e-smoke-m10`: fixture plugin `fixtures/plugins/m10-smoke` (skill `greet`, stdio server `echo` run with `node`, setting `GREETING`) added as a plugin dir, setting saved, Test lists the tool, enabled, assigned to an agent; a mock ticket run calls `ticket_get`, `skill_load`, `m10-smoke__echo`, `result_submit`; the tool-calls API shows all four `ok`, with `source = plugin` for the echo call and `skillsUsed = ["m10-smoke:greet"]`.
- Docs: "Adding an MCP transport" in `docs/architecture.md`; `AGENTS.md` status; M10 spec status and placeholder rule.

## Out of scope

Per-run plugin server instances, SSE transport, sandboxing and secret scoping (M11), plugin tool output pagination beyond the existing cap.

## Acceptance criteria

- [x] Plugin settings encrypted, write-only, keys derived from placeholders
- [x] stdio and HTTP plugin MCP servers proxied as `<plugin>__<tool>`, shared across runs, with restart/backoff, unhealthy, idle shutdown, list-changed refresh
- [x] A hung or failing plugin server never blocks core tools
- [x] Chat profiles see only `readOnlyHint` plugin tools; compaction sees none
- [x] Test button and stdio privilege warning
- [x] Tool calls and Skills Used visible per run; gateway tool names rendered in Claude, Cursor, Codex, and OpenCode consoles
- [x] `make test`, clippy, `make web-test`, `make e2e-smoke-m10`, `make e2e-smoke-m03` pass
