# Connector and Tool-Source Foundations Design

**Status:** Draft — awaiting review  
**Date:** 2026-10-01  
**Owner/reviewer:** Technical Lead  
**Milestone:** [M10 — Plugins](../../milestones/M10-plugins.md) (prerequisite for Part 2b)  
**Amends:** [M10 Plugins Design](2026-09-29-m10-plugins-design.md) — Part 2b builds on the extension points defined here.

## Decision summary

Before Part 2b adds plugin MCP tools, Coppice gets explicit extension points so that:

- **A new agent connector** is one descriptor entry, one adapter, and one config struct — no edits across the server, CLI, and web.
- **A new tool source or plugin capability** plugs into a registry — no new match arms in the gateway router.

This plan changes **no behavior**: same config file format, same tool names, same profile matrix, same per-run MCP files, same live console output. Existing tests and smokes are the safety net.

## Goals

- One source of truth for connector facts, shared by server, CLI, and web.
- One MCP server spec rendered into each connector's configuration style.
- One subprocess runner for CLI connectors (spawn, cancel, deadline, stderr, session id, final-text fallback).
- One tool registry in the gateway; core tools and skills become two sources; plugin MCP becomes a third in Part 2b.
- One parser per plugin capability; MCP server entries keep their full spec for the Part 2b proxy.

## Non-goals

- Declarative connectors (config- or manifest-defined CLIs driven by a generic adapter). The CLIs differ too much (per-run `HOME`, `opencode serve`, Codex `-c` flags).
- Changing connector config keys or options. Each connector keeps its typed `[agent.connectors.<id>]` section.
- Plugin MCP proxy, plugin settings, Test button, Tools & Skills tab, smoke m10 — Part 2b.
- Any new connector.

## Current state (baseline)

- `AgentProvider` (`id`, `run(AgentRunInput)`) and `McpAccess { url, token }` are shared. Everything else is per connector.
- Connector facts are string checks spread across the code: `READ_ONLY_CAPABLE_CONNECTORS`, `CHAT_RESUME_CONNECTORS` (`providers/mod.rs`), session-event connectors and resume connectors (`workers/job_worker.rs`), structured-console connectors (`api/ws/live.rs`), model validation (`services/agent_health.rs`, `api/connectors.rs`), the CLI's `ConnectorId` and `CONNECTORS` table (`cli/src/commands/connector/registry.rs`), and the web's live-view choice (`web/src/features/tickets/TicketDrawer.tsx`).
- Each CLI adapter (`claude_code.rs`, `codex.rs`, `cursor.rs`, `kilo_code.rs`) repeats spawn, stderr pump, deadline/cancel `select!`, session-id capture, and `extract_result_from_text`, and hand-writes its MCP config with its own copy of `"coppice"`.
- The gateway's tools are a closed `CoreTool` enum with `name()`, `definition()`, and `dispatch` matches. `run_tool_calls.source` is `core` or `skill`; `plugin` and `plugin_id` are never written. `ToolHost::list` is synchronous.
- `PluginManifest` has fixed `skills` and `mcp_servers` fields; `McpServerEntry` keeps only `name` and `kind` (command, args, env, URL, headers are discarded). Unsupported parts are directory names in `UNSUPPORTED_DIRS`.

## Architecture

```text
 connectors crate (static descriptors) ◄── cli (install, doctor, list)
          ▲
          │ lookups
 server ──┼── providers::registry (descriptor id → factory)
          ├── providers::cli_runner (CLI adapters: invocation + line handler)
          ├── mcp::wiring (McpServerSpec → per-style files / flags / env)
          ├── mcp::registry (ToolRegistry: Vec<Arc<dyn ToolSource>>)
          │      ├── CoreToolSource
          │      ├── SkillToolSource
          │      └── (Part 2b) PluginMcpSource
          └── plugins::capability (CapabilityParser per kind)
 web ── /api/connectors (console kind + caps) → live view choice
```

## Connector layer

### `connectors` crate

New workspace member `connectors/` (no dependency on `server` or `cli`; depends only on `serde`). It holds one static table:

```rust
pub struct ConnectorDescriptor {
    pub id: &'static str,                 // matches [agent.connectors.<id>] and agents.connector
    pub display_name: &'static str,
    pub binary: &'static str,             // "mock" for the built-in
    pub install: InstallInfo,             // auth_hint, auth_paths, auth_env (moved from the CLI table)
    pub default_model_providers: &'static [&'static str],
    pub mcp_wiring: McpWiring,
    pub mcp_tool_names: ToolNameStyle,
    pub console: ConsoleKind,
    pub caps: Capabilities,
}

pub enum McpWiring { ClaudeJson, CursorHome, OpenCodeJson, KiloJson, CodexFlags, MockHttp }
pub enum ToolNameStyle { McpDoubleUnderscore, Dash, Underscore, ServerToolFields, None }
pub enum ConsoleKind { OpenCodeSession, Structured, Plain }

pub struct Capabilities {
    pub read_only_tools: bool,   // adapter enforces a read-only allowlist when asked
    pub chat_resume: bool,       // chat turns may resume the CLI session
    pub session_events: bool,    // adapter reports a session id while running
    pub run_resume: bool,        // work_on_ticket runs may resume a prior session id
}

pub fn all() -> &'static [ConnectorDescriptor];
pub fn get(id: &str) -> Option<&'static ConnectorDescriptor>;
```

Baseline values (must equal today's behavior):

| id | mcp_wiring | mcp_tool_names | console | read_only_tools | chat_resume | session_events | run_resume |
|----|-----------|----------------|---------|:-:|:-:|:-:|:-:|
| `mock` | MockHttp | None | Plain | ✓ | ✓ | — | — |
| `claude-code` | ClaudeJson | McpDoubleUnderscore (`mcp__coppice__<tool>`) | Structured | ✓ | ✓ | ✓ | ✓ |
| `cursor` | CursorHome | Dash (`coppice-<tool>`) | Structured | ✓ | ✓ | ✓ | ✓ |
| `codex` | CodexFlags | ServerToolFields (`server`, `tool`) | Structured | — | ✓ | ✓ | — |
| `kilo-code` | KiloJson | Underscore | Structured | — | — | ✓ | — |
| `opencode` | OpenCodeJson | Underscore (`coppice_<tool>`) | OpenCodeSession | — | ✓ | ✓ | — |

The kilo-code tool-name style is unverified (no live CLI); `Underscore` follows its OpenCode fork. The value only affects console labels.

### Server use of the descriptor

Every connector-id string check outside adapter files becomes a descriptor lookup:

- `connector_enforces_read_only`, `connector_supports_chat_resume` → `caps`.
- `job_worker` session-event and resume checks → `caps.session_events`, `caps.run_resume`.
- `api/ws/live.rs` structured/opencode choice → `console`.
- Per-connector model listing and validation (`api/connectors.rs`, `services/agent_health.rs`) move behind a `ModelCatalog` trait returned by the adapter factory (below); the API and health worker call the trait.

A unit test scans `server/src` (excluding `providers/<adapter>.rs`, console parsers, model modules, and tests) for the literal connector ids and fails if any remain. The allowlist lives in the test.

### Registry

```rust
pub struct ConnectorFactory {
    pub id: &'static str,
    pub build: fn(&AppConfig, &FactoryDeps) -> Option<BuiltConnector>,  // None when disabled
}
pub struct BuiltConnector {
    pub provider: Arc<dyn AgentProvider>,
    pub models: Option<Arc<dyn ModelCatalog>>,
}
```

`ConnectorRegistry::from_config` iterates one `FACTORIES` list. A startup assertion (and unit test) checks every factory id has a descriptor and every descriptor has a factory. `model_providers_for` and the five `*_model_providers` fields go away.

### API and web

`GET /api/connectors` returns, per configured connector: `{ id, displayName, console, caps: { readOnlyTools, chatResume } }` (additive; `id` unchanged). The web keeps the list in its connectors query and `TicketDrawer` picks the live view from `console` (`OpenCodeSession` → `LiveSession`, `Structured` → `ClaudeLiveConsole`, `Plain` → `LiveConsole`). Runs whose connector is no longer configured fall back to `Plain`.

### CLI

`coppice connector …` uses `connectors::all()` / `get()`. `ConnectorId`, `ConnectorMeta`, and `CONNECTORS` are deleted. Output of `list`, `doctor`, `install`, `setup`, `enable` is unchanged.

### MCP wiring

`server/src/mcp/wiring.rs`:

```rust
pub struct McpServerSpec { pub name: &'static str /* "coppice" */, pub url: String, pub token_env: &'static str /* "COPPICE_MCP_TOKEN" */ }
impl McpServerSpec {
    pub fn from_access(access: &McpAccess) -> Self;
    pub fn claude_json(&self) -> String;              // run_dir/mcp.json body
    pub fn cursor_mcp_json(&self) -> String;          // <home>/.cursor/mcp.json body
    pub fn cursor_cli_config(&self) -> String;        // cli-config.json with Mcp(<name>:*) allow rule
    pub fn opencode_json(&self) -> String;
    pub fn kilo_json(&self) -> String;
    pub fn codex_args(&self) -> Vec<String>;          // -c mcp_servers.<name>.… pairs
    pub fn env(&self, token: &str) -> Vec<(&'static str, String)>;
}
```

Snapshot tests pin each renderer to the exact bytes adapters write today, then adapters switch to the renderer. The server name `"coppice"` and the token env name exist only here (and in `protocol::SERVER_NAME`, which the spec reuses).

### CLI runner

`server/src/providers/cli_runner.rs` owns the loop the four CLI adapters repeat:

```rust
pub struct CliInvocation { pub program: String, pub args: Vec<String>, pub env: Vec<(String, String)>, pub cwd: PathBuf, pub timeout: Duration }

pub trait LineHandler: Send {
    /// One stdout line. Return events for the console, an optional session id, and/or final text.
    fn on_line(&mut self, line: &str) -> LineOutcome;
    fn finish(&mut self) -> Option<String>; // final text when the stream ends
}

pub async fn run_cli(inv: CliInvocation, handler: &mut dyn LineHandler, io: RunIo) -> Result<String, ProviderError>;
// RunIo: stream handle, cancel_rx, session_created_tx
```

`run_cli` spawns with `kill_on_drop`, pumps stderr into the run stream as today, races deadline and cancel, forwards the first session id once, and returns final text; adapters still call `extract_result_from_text`. Each adapter keeps its argument building, per-run dirs (Cursor `HOME`), and its line handler (the existing console publishers become `LineHandler`s). OpenCode (serve process + HTTP events) and mock stay custom `AgentProvider`s that use only the descriptor and the wiring spec.

A test-only fake CLI binary (feature `embedded-test-db`, like `fake-opencode`) covers cancel, timeout, stderr capture, session-id capture, non-zero exit, and final-text fallback.

### Adding a connector (after this plan)

1. Descriptor entry in `connectors`.
2. Config struct + field in `AgentConnectorsConfig`.
3. Adapter: `CliInvocation` builder + `LineHandler` (or a custom `AgentProvider`), optional `ModelCatalog`.
4. Factory entry in `FACTORIES`.
5. Wiring renderer only if it needs a new `McpWiring` style.

`docs/architecture.md` carries this checklist.

## Gateway tool sources

### `ToolSource`

`server/src/mcp/source.rs`:

```rust
pub enum SourceKind { Core, Skill, Plugin }   // stored as run_tool_calls.source

pub struct SourcedTool {
    pub def: ToolDefinition,
    pub source: SourceKind,
    pub plugin_id: Option<Uuid>,
    pub key: String,          // source-private handle (e.g. the core tool id or "<server>/<tool>")
}

#[async_trait]
pub trait ToolSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool>;
    async fn call(&self, ctx: &ToolCtx<'_>, tool: &SourcedTool, args: Value) -> Result<ToolResult, ToolError>;
}

pub enum ToolContent { Text(String), Image { data: String, mime_type: String } }
pub struct ToolResult { pub content: Vec<ToolContent>, pub is_error: bool }
```

- `CoreToolSource`: the current `CoreTool` enum, `core_tools_for(profile)`, and handlers, moved unchanged (minus `skill_list`/`skill_load`). Handlers keep returning JSON; the source wraps it as one text block.
- `SkillToolSource`: `skill_list` / `skill_load` over `SkillCatalog`, scoped by `scope.plugin_ids` as today. Listed for every profile (matches the current matrix).
- Source order: Core, Skill, then (Part 2b) Plugin.

### `ToolRegistry` (router)

`server/src/mcp/registry.rs`, built once at startup and held in `AppState`. `RunToolHost` asks it for the token's tool set and dispatches through it. The router alone owns:

- **Merge:** concatenate sources in order; on duplicate name keep the first and log a warning once per name.
- **Profile read-only filter:** for `human_chat` and `conversation`, drop tools whose `read_only` is false, except `result_submit` (the reply path). Core tools already carry correct hints, so the core list for these profiles is unchanged; this rule exists for plugin tools.
- **Denial:** a name not in the merged set → tool error `denied: tool "<name>" is not available for this run`, logged with `source = core` (unchanged text).
- **Limits:** per-call timeout, output cap with the existing truncation suffix (applied to text blocks).
- **Logging:** `run_tool_calls` gets `source` from the tool and `plugin_id` when set.

`ToolHost::list` becomes `async fn list(&self) -> Vec<ToolDefinition>`; `ToolHost::call` returns `ToolResult`; `protocol.rs` renders content blocks (`{type:"text",text}` / `{type:"image",data,mimeType}`).

### Adding a tool source (after this plan)

Implement `ToolSource`, register it in the registry builder. Router, tokens, protocol, and logging are untouched.

## Plugin capabilities

`server/src/plugins/capability.rs`:

```rust
pub enum CapabilityOutcome<T> { Absent, Supported(T), Unsupported(String /* reason */) }

pub trait CapabilityParser {
    type Output;
    const KEY: &'static str;   // "skills", "mcpServers", "commands", "agents", "hooks"
    fn parse(root: &Path, plugin_json: Option<&Value>, layout: PluginLayout) -> CapabilityOutcome<Self::Output>;
}
```

- `SkillsCapability` — current skills logic (overrides, containment, frontmatter), unchanged.
- `McpServersCapability` — parses `.mcp.json` (and `plugin.json` `mcpServers` inline object, as Claude Code allows) into full entries:

  ```rust
  pub struct McpServerEntry { pub name: String, pub transport: McpServerTransport, pub error: Option<String> }
  pub enum McpServerTransport {
      Stdio { command: String, args: Vec<String>, env: BTreeMap<String, String> },
      Http { url: String, headers: BTreeMap<String, String> },
      Unsupported { kind: String },   // sse, unknown
  }
  ```

  Values keep their `${…}` placeholders (substituted only when Part 2b starts a server). A malformed `.mcp.json` keeps today's ruling: the plugin is `invalid`.
- `CommandsCapability`, `AgentsCapability`, `HooksCapability` — report `Unsupported("not supported yet")` when the directory exists.

`parse_plugin` calls each parser and assembles the typed `PluginManifest` (`skills`, `mcp_servers`, `unsupported: Vec<UnsupportedPart { key, reason }>`). Old stored manifests (`mcpServers[].kind`, `unsupported: [String]`) still deserialize via serde defaults/untagged fallback until the startup rescan rewrites them. The web plugin schema accepts both shapes for one release.

### Adding a plugin capability (after this plan)

1. A `CapabilityParser` and a manifest field.
2. Optionally a `ToolSource` that serves it at run time (scoped by `scope.plugin_ids`).
3. Plugin card renders the new section.

## Error handling

Unchanged user-visible errors. New internal failures:

- Descriptor/factory mismatch → startup panic with the missing id (caught by unit test first).
- Duplicate tool names across sources → warning log; first source wins.
- Old-shape manifest that cannot be read → plugin shows as `invalid` until rescan, same as a corrupt manifest today.

## Testing

### Unit

- Descriptor table: ids unique; every factory has a descriptor and vice versa; baseline capability values (table above).
- Connector-literal scan test (allowlist).
- Wiring renderer snapshots equal today's bytes (claude, cursor mcp + cli-config, opencode, kilo, codex args, env).
- `run_cli` with the fake CLI: cancel, timeout, stderr, session id once, non-zero exit, final text.
- `ToolRegistry` with fake sources: merge order, duplicate names, chat read-only filter (keeps `result_submit`), denial text, timeout, truncation, `source`/`plugin_id` logged.
- Capability parsers: each kind; `mcpServers` stdio/http/sse/unknown round-trip with placeholders preserved; inline `plugin.json` `mcpServers`; old manifest JSON deserializes.

### Integration (embedded Postgres)

- Existing `integration_mcp.rs`, `integration_plugins.rs`, provider, chat, and job-worker suites pass unchanged.
- `/api/connectors` returns `console` and `caps`.
- A run's `run_tool_calls` rows carry `source = skill` for `skill_load` and `core` otherwise.

### Web

- `TicketDrawer` chooses the live view from the connectors query `console`; unknown connector → plain.

### Smoke

`make e2e-smoke-m03` (on a fresh database) and `make e2e-smoke-m09` pass. A manual OpenCode run on the default stack still uses its per-run serve and calls `coppice_*` tools.

## Delivery order (plan F)

1. `connectors` crate + descriptors; CLI migrated.
2. Server descriptor lookups, `FACTORIES` registry, `ModelCatalog`, literal-scan test; `/api/connectors` `console` + `caps`.
3. Web live-view choice from `console`.
4. `McpServerSpec` renderers (snapshots first), adapters switched.
5. `cli_runner` + fake CLI; claude-code, codex, cursor, kilo-code migrated one at a time.
6. `ToolSource`, `ToolRegistry`, core + skill sources, async `list`, content blocks, `source`/`plugin_id` logging.
7. Capability parsers; full `McpServerEntry`; manifest back-compat; web schema accepts both shapes.
8. Docs (`docs/architecture.md` checklists, `docs/providers/README.md`), final verification.

## Risks

| Risk | Mitigation |
|------|------------|
| Refactor silently changes a connector's CLI invocation or MCP file | Renderer snapshots pin bytes; adapters migrated one per task with their existing tests |
| Runner changes cancel/timeout semantics | Fake-CLI tests for each path before migrating adapters |
| Tool registry changes the visible tool list | Existing gateway tests unchanged; explicit test that core lists per profile are identical |
| Old stored manifests break the plugin page | Serde fallback + startup rescan; web schema accepts both shapes |
| Scope creep into Part 2b | Proxy, settings, Test button, Tools & Skills tab stay in Part 2b |

## Acceptance criteria

- [ ] No connector-id string checks outside adapters (scan test passes)
- [ ] CLI uses the shared descriptor table; output unchanged
- [ ] Web live view chosen from `/api/connectors` `console`
- [ ] All CLI adapters use `McpServerSpec` renderers and `run_cli`
- [ ] Gateway dispatches through `ToolRegistry`; `run_tool_calls.source`/`plugin_id` populated
- [ ] Plugin manifests built from capability parsers; MCP server entries keep full spec
- [ ] `make test`, clippy, `make web-test`, `make e2e-smoke-m03` (fresh DB), `make e2e-smoke-m09` pass
