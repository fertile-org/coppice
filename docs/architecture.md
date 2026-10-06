# Architecture

## Overview

Coppice is a monorepo with three deliverables and shared deploy/test tooling:

```text
server/   Rust API — Axum, SQLx, Tokio
web/      React SPA — Vite, TanStack Query, Tailwind
cli/      Rust operator CLI (workspace member)
connectors/  Static connector descriptors shared by server, CLI, and web (via the API)
desktop/  Electron shell + electron-builder packaging (M11)
deploy/   Docker Compose, Dockerfiles, default config
```

Rust workspace: root `Cargo.toml` with members `config`, `connectors`, `db-migrations`, `server`, `cli`. `web/` is an independent Node package.

## Server layers

```text
server/src/
  api/          HTTP routes, request/response DTOs (thin handlers)
  services/     Business logic, DB queries, validation orchestration
  domain/       Entity types, enums, pure validation helpers
  db/           Pool setup, migration runner
  knowledge/    Full-text retrieval, compaction context and candidate contract (M06)
  mcp/          MCP gateway at /mcp: per-run tokens, tool catalog, tool handlers (M10)
  middleware/   Session auth, CSRF, admin checks
  providers/    AgentProvider trait, connector factories, CLI runner, mock / opencode / claude-code / codex / cursor / kilo-code adapters
  plugins/      Plugin folder parsing (capability parsers), discovery, skill catalog (M10)
  workers/      In-process Tokio job workers (M03)
  storage/      Filesystem artifact store (attachments)
  config/       Figment-based AppConfig
  desktop/      `coppice-server desktop`: data dir, secrets, bundled Postgres, bounded shutdown (M11)
```

**Request flow:** `api/*` → `services/*` → SQLx / `storage/*`. Handlers extract auth via `AuthUser`, get a pool from `AppState`, call a service, map errors to HTTP status.

**Error pattern:** Services return typed errors (`thiserror`, e.g. `TicketError`). API maps them to `StatusCode` + JSON message. Use `anyhow` only at CLI/worker boundaries.

**State:** `AppState` holds `AppConfig`, optional `PgPool`, and `AttachmentStore`. Router built in `lib.rs::app()`.

## Domain conventions

- DB columns and Rust enums use `snake_case` (e.g. `in_progress`, `waiting_for_agent`).
- JSON API responses use `camelCase` (`#[serde(rename_all = "camelCase")]` on DTOs).
- IDs are `Uuid` everywhere.
- Timestamps: `time::OffsetDateTime`, serialized as RFC3339 strings in API.
- Ticket status/substatus validation lives in `domain/substatus.rs` and `domain/ticket.rs` — keep rules there, not in handlers.

## Database

- Plain PostgreSQL 16 (no pgvector; `unaccent` comes from contrib).
- Migrations: `server/migrations/*.sql`, applied by `coppice migrate` and on test connect (`db::connect_and_migrate`). Migrations 001 and 013 were rewritten in place to drop pgvector; `coppice_migrations::run_migrations` (crate `db-migrations/`, shared by server and CLI) updates their recorded checksums on older databases before sqlx runs.
- No Redis; agent job queue uses Postgres `agent_jobs` (M03).
- **M03 tables:** `agent_runs` (one row per ticket+agent execution; statuses `queued`/`running`/`completed`/`failed`/`cancelled`; unique partial index on active `(ticket_id, agent_id)`), `agent_jobs` (queue row per run; `FOR UPDATE SKIP LOCKED` claim by workers).
- **M06 tables:** `knowledge_items` (mutable lifecycle pointer), immutable `knowledge_revisions` (generated `search_vector` + GIN index), `knowledge_item_sources` (source tickets), `knowledge_usage_logs` (unique run/revision audit snapshot with full-text `score`), `workspace_settings` (compaction agent), and the compaction tables `knowledge_compaction_queue`, `knowledge_compaction_batches` (at most one queued/running), `knowledge_compaction_batch_tickets`. Compaction runs are ordinary `agent_runs` rows with `compaction_batch_id` set.
- **Connector checks:** `connector_checks` (Test connection; at most one `queued`/`running` per connector). A check's run is an `agent_runs` row with `connector_check_id` set; every run has exactly one owner (`ticket_id`, `chat_session_id`, `compaction_batch_id`, or `connector_check_id`).

## Auth

- Session cookie (httpOnly), argon2 password hashes.
- Public routes: `/health`, `/api/auth/login`, `/api/auth/bootstrap`.
- Protected routes: session middleware + CSRF on mutations (`X-CSRF-Token` from login response).
- Roles: `admin` / `member` — admin-only routes use `middleware/admin.rs`.

## Agent execution (M03)

All agent execution goes through `AgentProvider`; orchestration lives in services + workers:

```text
providers/mod.rs          trait + AgentRunResult contract
providers/registry.rs     FACTORIES + ConnectorRegistry — builds providers and model catalogs from config
providers/models.rs       ModelCatalog per connector (configured model providers, model listing)
providers/cli_runner.rs   shared subprocess loop for the streaming-JSON CLI adapters
providers/mock.rs         deterministic fixtures from fixtures/agent-responses/
providers/opencode.rs     HTTP serve-mode connector (host testing, API keys)
providers/claude_code.rs  subprocess connector (claude -p, host-managed auth)
providers/codex.rs         subprocess connector (codex exec, host-managed auth)
providers/cursor.rs        subprocess connector (agent -p, host-managed auth)
providers/kilo_code.rs     subprocess connector (kilo run, host-managed auth)
services/run_service.rs   create/cancel/finish runs
services/job_service.rs   enqueue, claim (SKIP LOCKED), mark done/failed
services/repo_service.rs       global registered repos (local_path, verify)
services/worktree_service.rs   worktree per (ticket, agent) from registered local_path
services/context_builder.rs    write the slim tool-first .agent/context.md (agent, task, repo, skills, tool pointers)
services/result_contract.rs    apply nextStatus, comments, blocker metadata
workers/job_worker.rs     poll queue, run pipeline, spawn at server startup
```

**Registered repositories:** Admin registers operator-managed git checkouts via `local_path` (instance-wide). Optional `remote_url` for display and future PR APIs. Coppice does **not** `git clone`. See [M03 registered repositories spec](superpowers/specs/2026-06-08-m03-registered-repositories-design.md).

**Run pipeline (worker):** claim pending job → load run/ticket/agent/repo → validate repo `local_path` → mark running → ensure worktree from registered path (`WORKTREES_PATH/TICKET-{id}-{agent}-{repo}/`) → write context file → mint a gateway token → call `AgentProvider::run` → apply result contract → revoke the token → finish run.

## Connector layer

Connector facts live in one static table, `connectors/src/lib.rs` (`coppice_connectors::all()` / `get(id)`, id constants such as `MOCK`). Each `ConnectorDescriptor` carries the id (matches `[agent.connectors.<id>]` and `agents.connector`), display name, binary, install/auth hints, default model providers, `mcp_wiring` (`McpWiring`), `mcp_tool_names` (`ToolNameStyle`, console labels only), `console` (`ConsoleKind`: `OpenCodeSession` / `Structured` / `Plain`), and `caps` (`read_only_tools`, `chat_resume`, `session_events`, `run_resume`, `run_server`). The crate depends only on `serde`.

- **Server.** Behavior checks read the descriptor (`caps`, `console`), never a connector-id string. A unit test in `providers/registry.rs` fails on connector-id literals in `server/src` outside `providers/`, `sessions/opencode*`, and tests.
- **Registry.** `providers/registry.rs` has one `FACTORIES` list of `ConnectorFactory { id, build }`; `build(&AppConfig, &FactoryDeps)` returns `None` when the connector is disabled, else `BuiltConnector { provider, models }`. Startup asserts factories and descriptors match one-to-one.
- **Models.** `ModelCatalog` (`providers/models.rs`) reports `model_providers()` from config and `list_models(model_provider)`. Agent health checks that an agent's `model_provider` is in `model_providers()` unless `checks_model_provider()` is false (mock). `GET …/models` maps errors to 502 `"{id} models: {err}"`.
- **API / web.** `GET /api/connectors` returns `{ id, displayName, console, caps: { readOnlyTools, chatResume } }` per configured connector; `TicketDrawer` picks the live view from `console` (unknown connector → plain). The web schema parses an unknown `console` value as `plain` rather than rejecting the list; the wire strings (`openCodeSession`, `structured`, `plain`) are pinned by a test in the connectors crate.
- **CLI.** `coppice connector …` iterates `coppice_connectors::all()` / `get()` for ids, binaries, and auth hints. `doctor` and `list` run the shared probe library (below), so they need no per-connector code; config section, install steps, and setup are still matched by id in `cli/src/commands/connector/` — see the checklist below.
- **Probes.** `coppice_connectors::probe` (std only) answers "installed, logged in, probe ok?" for one descriptor: `probe(descriptor, &ProbeEnv { home, path, command_override, env_lookup }, timeout)` → `ProbeReport { binary, auth_env_set, auth_paths_found, probe, probe_proves_auth }`. The binary is the config `command` when set, else the descriptor `binary`; bare names are searched on `path`. The probe command is the descriptor's `install.probe_args` (cursor `models`, opencode `auth list`, the rest `--version`), run with the server's environment except `PATH` and `HOME`, which come from `ProbeEnv`; stdin null, killed at the timeout. Output is capped (first line ≤ 200 chars on success, message ≤ 500 on failure) with any auth env value redacted; the report carries env **names** and HOME-relative auth path names only. `install.probe_proves_auth` (cursor, opencode) marks probes that only succeed when logged in. `install.docs_url` is the vendor install link shown on the page.
- **Server PATH.** `main.rs` calls `augment_path(HOME, PATH)` before the Tokio runtime starts, prepending each existing directory in `COMMON_BIN_DIRS_HOME` (`~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`) and `COMMON_BIN_DIRS_ABS` (`/opt/homebrew/bin`, `/usr/local/bin`) that is not already on PATH. Desktop launchers often omit these; because the whole process uses the result, a binary the page reports as found is the one real runs spawn.
- **Diagnostics (Tools → Connectors).** `services/connector_probe_service.rs` keeps probe results in memory (`AppState.connector_probes`), filled for every connector except `mock` by a startup task and refreshed per connector by Run check; probes run in `spawn_blocking` with a 10 s timeout. Disabled connectors are probed too. A connector whose startup probe has not finished reports `probedAt: null` (`cli.found: false`, probe `not_run`). Each status also carries the latest finished non-check run of an agent on that connector (with whether it made an `ok` `ticket_get` / `result_submit` call) and the latest connector check. Routes, all admin-only (`mock` and unknown ids → 404, POSTs need `X-CSRF-Token`): `GET /api/tools/connectors`, `POST /api/tools/connectors/{id}/check`, `POST /api/tools/connectors/{id}/test` → `{ checkId, runId }`, `GET /api/tools/connector-checks/{id}`.
- **Check runs (Test connection).** A `connector_checks` row (migration `032_connector_checks.sql`; `queued` / `running` / `passed` / `failed`) owns one `agent_runs` row (`connector_check_id`, `job_type = connector_check`, profile `connector_check`, no plugins), the way a compaction batch owns its run. `services/connector_check_service.rs` validates that the agent uses the connector and that the connector is enabled (400 otherwise) and allows one active check per connector (unique index; a second start → 409). `workers/job_worker/connector_check.rs` runs the agent's provider in a scratch dir `<artifacts_dir>/runs/<run id>/check/` holding only a fixed `.agent/context.md`, with a timeout of `min(connector run timeout, 180 s)`, then deletes the dir. No ticket, comment, workflow, notification, knowledge, or repository is touched. The check **passes** when the run succeeds, `run_tool_calls` has an `ok` `ticket_get` and an `ok` `result_submit`, and the outcome is `done`; otherwise it **fails** with the first reason that applies: the run error (e.g. `mcp_unavailable`, a timeout), `ticket_get was not called`, `result_submit was not called`, or `result was <outcome>`. `result_submit` accepts any outcome in this profile and the **first** valid submission wins (malformed arguments are rejected before storing; later submissions are denied), so a check cannot be rescued by resubmitting. Failure text is capped at 500 chars with the run token and connector auth env values redacted. At startup, checks left `queued` / `running` by a previous process are failed with `server restarted`.
- **MCP wiring.** `mcp/wiring.rs` `McpServerSpec::from_access` renders the gateway entry in each style (`claude_json`, `cursor_mcp_json`, `cursor_cli_config`, `opencode_json`, `kilo_json`, `codex_args`). The server name `coppice` and token env `COPPICE_MCP_TOKEN` live only there and in `mcp/grant.rs` (`McpAccess::env`).
- **CLI runner.** `providers/cli_runner.rs` `run_cli(CliInvocation, &mut dyn LineHandler, RunIo)` (or `run_cli_with_stdin` to feed a prompt on stdin) spawns the CLI in its own session (`setsid`), mirrors stderr to the adapter's tracing target (keeping the first 40 lines), races cancel and deadline, forwards the first session id, and returns `CliExit` or `CliError`. When the run ends, is cancelled, or times out, the runner SIGTERMs that process group and SIGKILLs it after 3 s, so shells and dev servers the CLI backgrounded die with it. The pid, pgid, start token, and command are stored in `{artifacts_dir}/agent-processes.json`. Startup reaps records left by a crash and skips a pid whose start token or command no longer matches. Stop lines go to `agent-process.log` and `runs/<id>/process-stops.log` (`coppice::agent_process`). Adapters keep their own error wording by mapping `CliExit` / `CliError`, then read final text from their `LineHandler`. OpenCode's per-run `serve` uses the same registry. Mock does not spawn a process. A grandchild that calls `setsid` itself leaves the group and is not tracked.

### Adding a connector

1. Descriptor entry in `connectors/src/lib.rs` (plus an id constant), including `install.probe_args`, `probe_proves_auth`, and `docs_url` — the CLI `doctor` and the Tools → Connectors page need nothing else.
2. Config struct + field in `AgentConnectorsConfig` (`config/src/lib.rs`) and an arm in `AgentConnectorsConfig::enabled(id)` (a config test fails for any descriptor id without one; `coppice connector list` reads it), and an `[agent.connectors.<id>]` section in `config.example.toml` and `deploy/config/config.example.toml`.
3. Adapter in `server/src/providers/`: a `CliInvocation` builder + `LineHandler` driven by `run_cli` (or a custom `AgentProvider`), and a `ModelCatalog`. A structured console publisher must type its events `<name>.console.<kind>` (e.g. `codex.console.tool`); the worker persists exactly that shape for replay.
4. Factory entry in `FACTORIES`.
5. A `McpServerSpec` renderer only if it needs a new `McpWiring` style.
6. If `caps.read_only_tools` is true, add it to `READ_ONLY_CAPABLE_CONNECTORS` (`providers/mod.rs`; a test checks it matches the descriptors).
7. CLI arms in `cli/src/commands/connector/`: `enable.rs` (connectors that get a default `command`), `install.rs`, and `setup.rs` (per-id install and setup steps).
8. A doc in `docs/providers/` and a row in its README.

The connector-id literal scan in `providers/registry.rs` derives its ids from `coppice_connectors::all()`, so it needs no edit.

A new `ConsoleKind` also needs branches in `web/src/lib/schemas/connector.ts`, `web/src/features/runs/LiveRunView.tsx`, and `server/src/api/ws/live.rs`.

## MCP gateway (M10)

Runs are **tool-first**: `.agent/context.md` says who the agent is and what the task is, and everything else is pulled through tools instead of being embedded.

```text
mcp/server.rs      streamable HTTP endpoint at POST/GET /mcp (token auth, not session/CSRF)
mcp/token.rs       mint, verify and revoke per-run tokens
mcp/grant.rs       McpAccess (url + token) and the RunToolGrant lifetime guard
mcp/catalog.rs     core tools per context profile (profile matrix)
mcp/source.rs      ToolSource trait; CoreToolSource, SkillToolSource
mcp/registry.rs    ToolRegistry: merge sources, profile filter, denial, limits, call logging
mcp/host.rs        RunToolHost: one token's view of the registry (ToolHost impl)
mcp/wiring.rs      McpServerSpec rendered per connector wiring style
mcp/proxy/         plugin MCP servers: McpTransport seam, stdio + HTTP, shared pool, PluginMcpSource
mcp/tools/         ticket_get, ticket_comments, ticket_runs, board_agents, knowledge_search,
                   comment_post, skill_list, skill_load, result_submit
```

**Profile → tool matrix** (`mcp/catalog.rs` `core_tools_for`; skills come from `SkillToolSource`, plugin tools from `PluginMcpSource`):

| Tool | `full` / `human_agent` | `human_chat` / `conversation` | `knowledge_compaction` | `connector_check` |
|------|:-:|:-:|:-:|:-:|
| `ticket_get` | ✓ | ✓ | ✓ (batch tickets) | ✓ (fixed synthetic ticket, no DB read) |
| `ticket_comments`, `ticket_runs` | ✓ | ✓ | ✓ (batch tickets) | — |
| `ticket_search`, `board_agents` | ✓ | ✓ | — | — |
| `knowledge_search` | ✓ | ✓ | ✓ | — |
| `comment_post` | ✓ | — | — | — |
| `result_submit` | ✓ | ✓ (reply) | ✓ (`done` + candidates) | ✓ (any outcome; first valid wins) |
| `skill_list`, `skill_load` | ✓ | ✓ | ✓ | — |
| Plugin tools | assigned | assigned, `readOnlyHint` only | — | — |

**Tool sources.** Tools come from an ordered list of `ToolSource`s (`kind`, async `list(scope)`, `call(ctx, tool, args)` → `ToolResult` of text/image content blocks). `ToolRegistry::builtin()` registers Core, then Skill; `AppState::build_tool_registry` (held in `AppState::tools`) adds `PluginMcpSource` third when a database is present. `ToolRegistry` bounds each source's `list` by a list timeout (`[plugins] mcp_list_timeout_secs`, default 10 s): a source that times out contributes no tools for that request and is logged, while core tools are unaffected. `ToolRegistry::call(state, scope, name, args)` is the only router: it merges sources (duplicate name → first wins, warned once), drops non-read-only tools for `human_chat` / `conversation` except `result_submit`, denies unknown names with `denied: tool "<name>" is not available for this run`, applies `mcp.call_timeout_secs` and `mcp.max_output_bytes`, and logs each call to `run_tool_calls` with `source` (`core` / `skill` / `plugin`) and `plugin_id` for plugin-owned tools.

### Adding a tool source

Implement `ToolSource` in `mcp/` and add it to the list in `ToolRegistry::builtin()`. `builtin()` takes no arguments, so a source that needs runtime handles is constructed where the registry is built (`AppState::build_tool_registry(db, secret_store, plugin_mcp, list_timeout)` in `lib.rs`, which adds DB-backed sources such as `PluginMcpSource`; used by `main.rs` and `server/tests/common/mod.rs`) and passed to `ToolRegistry::new`. A new `SourceKind` also needs a migration widening the `run_tool_calls.source` check (`'core', 'skill', 'plugin'` in `028_mcp_gateway.sql`). Router, tokens, protocol, and logging stay untouched. Sources that do I/O in `list` (e.g. a plugin MCP server) should bound it, since every `tools/list` and call resolves all sources.

The gateway is authenticated by a per-run bearer token, minted when the run starts and revoked when it finishes, fails, or is stopped. Base URL comes from `mcp.base_url` (default `http://127.0.0.1:<server.port>/mcp`), which is correct in Docker, on the desktop, and in the cloud because the CLIs run beside the server.

Connectors receive the token as `COPPICE_MCP_TOKEN` and configure the gateway per run only — flags, per-process env, or a file under `<artifacts_dir>/runs/<run id>/`. Never the worktree, a registered repo checkout, or the operator's global CLI config. A connector that cannot be configured fails with `mcp_unavailable`. Per-connector mechanisms and verification status: [docs/providers/README.md](providers/README.md).

Agents finish with `result_submit`; a submitted result wins over a final JSON blob in the transcript.

**Observability.** `GET /api/agent-runs/{id}/tool-calls` returns the run's `run_tool_calls` oldest first (`tool`, `source`, `pluginId`, `pluginName`, `status`, `error`, `durationMs`, `argsSummary`) plus `skillsUsed` (distinct `name` of successful `skill_load` calls, first-use order). The ticket Runs tab shows them in the run row's **Tools & Skills** tab next to **Knowledge Used**. Live consoles title gateway calls `coppice · <tool>` or `<plugin> · <tool>` using `coppice_connectors::gateway_tool` and the connector's `ToolNameStyle`.

### Plugin MCP proxy

Plugin MCP servers (`.mcp.json` stdio and streamable HTTP entries) are proxied through the gateway as `<plugin>__<tool>`. `rmcp` is used only as a client inside the transports. The user-facing guide (plugin format, settings, writing a plugin) is [plugins.md](plugins.md); `examples/plugins/hello-coppice` is a working example covered by `integration_plugins.rs`.

```text
mcp/proxy/transport.rs  McpTransport (kind, connect) → McpConnection; Transports registry; ProxyError
mcp/proxy/stdio.rs      StdioTransport: child process, minimal env, stderr → tracing (coppice::plugin_mcp)
mcp/proxy/http.rs       HttpTransport: streamable HTTP with resolved headers
mcp/proxy/pool.rs       McpServerPool: one shared instance per (plugin_id, server) across runs
mcp/proxy/source.rs     PluginMcpSource (third ToolSource) + DbPluginServerCatalog
mcp/proxy/naming.rs     exposed tool names
plugins/placeholders.rs pure ${VAR} resolution (resolve, placeholder_keys, setting_sources)
services/plugin_settings_service.rs  encrypted, write-only plugin settings
```

- **Pool.** Lazy start on first `tools` / `call` / `test`, in a spawned task bounded by `mcp_start_timeout_secs` (default 20) so a caller's timeout never cancels a start. Each instance is keyed by a fingerprint of the resolved transport; a different fingerprint (settings or manifest changed) restarts it. Health: `stopped | starting | ready | backoff | unhealthy`. A failed start or closed connection is a failure with exponential backoff (1 s doubling to 60 s); three in a row → `unhealthy` (tools hidden, calls fail) until a Test or a fingerprint change. A placeholder error is `unhealthy` at once. The tool cache refreshes on `notifications/tools/list_changed` and on restart. A reaper stops instances idle for `mcp_idle_shutdown_secs` (default 600). Disabling or git-updating a plugin calls `stop_plugin`; so does a Test of a disabled plugin once it has its results. After a rescan, dir add/move/remove, or git job, servers of plugins no longer `ok` (missing, invalid, shadowed, removed) are stopped (`retain_plugins`). Graceful shutdown calls `shutdown_all`. Concurrent calls share one connection.
- **Source.** `PluginMcpSource` lists only plugins in the token's `plugin_ids` snapshot that are still enabled and `ok`, with decrypted settings; nothing for `knowledge_compaction`. Unsupported, unhealthy, and failing servers are skipped (logged). Descriptions and `readOnlyHint` pass through, so chat profiles see only read-only plugin tools via the registry's filter. Calls are logged with `source = plugin` and `plugin_id`. A call to a down server returns `plugin "<name>" unavailable` (plus a key-only reason such as `: missing setting "X"` or `: unsupported transport "sse"`).
- **Names.** Each part of `<plugin>__<tool>` is sanitized to `[A-Za-z0-9_-]`; names over 50 chars become the first 41 chars + `_` + 8 hex chars of SHA-256 of the unsanitized name, so `mcp__coppice__<name>` stays within 64.
- **Settings.** Keys are the `${VAR}` names used in an entry's command, args, env values, url, and header values (excluding `CLAUDE_PLUGIN_ROOT`); unknown keys are rejected. Values are stored with `SecretService` as `plugin-setting-<plugin_id>-<key>` and are write-only: the API returns `settings: [{ key, configured, source }]`. `source` is where the value would come from at start, from presence only (never the value): `setting` (stored; `configured` is true exactly then), `env` (an allowed, non-empty server environment variable, same rule as resolution), `default` (every reference has `:-default`), or `missing`. Deleting a `plugin_settings` row (directly or via the plugin cascade) deletes its secret (trigger).
- **Placeholders.** Resolved only when a server starts; stored manifests keep them. `${CLAUDE_PLUGIN_ROOT}` → plugin root. `${NAME}` / `${NAME:-default}` → plugin setting, else server environment (never names starting with `COPPICE_`, nor `DATABASE_URL` / `SECRETS_MASTER_KEY`), else `default`, else the start fails with `missing setting "NAME"`. An unterminated `${` is kept verbatim.
- **stdio children.** Environment cleared, then `PATH`, `HOME`, `LANG`, `TMPDIR` from the server when set, then the entry's resolved `env`; cwd is the plugin root; `kill_on_drop`. They run with the server's privileges until M13, so enabling or testing a disabled plugin with stdio servers asks for confirmation. The same confirmation names any `env`-sourced setting keys (keys only), since the server's environment values will be sent to the plugin; both warnings share one dialog.
- **No secret leaves the server.** Plugin responses, Test results, tool results, logs, and errors never contain command, args, env, URL, header, or setting values (`ResolvedTransport`'s `Debug` prints kind and key names only).
- **Test.** `POST /api/plugins/{id}/test` (admin, allowed while disabled) restarts every server of the plugin and returns per-server `status` (`ok | error | unsupported`) and tools; success clears `unhealthy`. Plugin responses carry `mcpServers[].health`.

### Adding an MCP transport

1. Implement `McpTransport` (`kind()`, `connect(spec, cwd)` completing the MCP `initialize` handshake) and its `McpConnection` in `mcp/proxy/`. Error text must never include resolved env values, header values, or credentialed URLs.
2. Register it in `Transports::builtin()` (`mcp/proxy/transport.rs`).
3. If the kind is new to manifests, add the variant to `McpServerTransport` (`plugins/capability.rs`, parsed from `.mcp.json`) and to `ResolvedTransport` + `resolve` / `transport_values` (`plugins/placeholders.rs`); the pool picks the transport by `ResolvedTransport::kind()`.
4. Test it against a loopback stub server, as `server/tests/integration_mcp_transport.rs` does for stdio (`server/tests/support/fake_mcp.rs`) and HTTP — no network beyond loopback.

## Plugins and skills (M10)

Plugins are Claude Code / Cursor format folders, `npx skills` style skills repos, or Claude Code marketplaces (one row per entry), loaded unchanged ([design](superpowers/specs/2026-10-04-standard-skills-and-marketplaces-design.md)). Parsing and filesystem work live in `plugins/`; state, rules, and enablement live in the service.

```text
plugins/manifest.rs         parse_plugin: plugin.json + one CapabilityParser per capability
plugins/capability.rs       skills, mcpServers (full stdio/http spec), agents/commands/hooks (unsupported)
plugins/skill_walk.rs       npx-skills compatible SKILL.md walker, skill containers, skills-package detection
plugins/marketplace.rs      expand .claude-plugin/marketplace.json into one row per entry (in-repo or external)
plugins/discover.rs         scan one plugin dir (the dir itself, else each direct child)
plugins/git_install.rs      URL allowlist + shallow clone (the only server-side git clone)
plugins/skills.rs           SkillCatalog: served skills of enabled plugins, ids `<plugin>:<skill>`
plugins/builtin.rs          embedded built-in skills written to mcp.builtin_plugins_dir on start
services/plugin_service.rs  plugin dirs, rescan + shadowing, enable, installs, agent assignment
api/plugins.rs              /api/plugin-dirs, /api/plugins, /api/plugin-installs, /api/agents/{id}/plugins
sessions/opencode_run_server.rs  one `opencode serve` per OpenCode run
```

- **Dirs and scan.** Admin-ordered plugin dirs; the default dir (`[plugins] dir`) always exists. A rescan upserts one row per discovered plugin, keyed by `(plugin_dir_id, rel_path)`; a name already found in an earlier dir is `shadowed`, a vanished row is `missing`, a bad manifest is `invalid`, a remote marketplace entry is `external`. Only `enabled` + `ok` plugins are assignable or served.
- **Discovery.** `discover(dir)` reads `dir` itself (rel_path `""`) if `is_plugin_dir_root(dir)`, else each direct child for which `is_plugin_root(child)` holds. A folder is one of three layouts, checked in order:
  1. **Marketplace** (`.claude-plugin/marketplace.json`): `expand_marketplace` yields one row per `plugins` entry instead of the folder. A string `source` is normalized (`./` stripped; absolute or `..` → `source must stay inside the repository`, also when the canonical target leaves the folder), must be a directory (`source path does not exist`), and must hold a `plugin.json` or a skills package (`no plugin or skills found at source`); it is parsed as rule 2 or 3 with rel_path `<folder rel>/<source>` and named by its `plugin.json` name, else the entry name (`parse_plugin_named`). An object `source` yields an `external` row (`manifest.external = { kind, url }`: `github` → `https://github.com/<repo>.git`, `git-subdir` → its `url` with `owner/repo` shorthand expanded and kind `git-subdir:<path>`, `url` → its `url`, others → no URL). Any other `source` → `source must be a path or an object`; an empty, invalid, or reserved entry name → `invalid plugin name`. Entry errors are prefixed `marketplace entry "<name>": `, and external and error rows use rel_path `<folder rel>#<entry name>`. Bad JSON or a non-array `plugins` → one `invalid` row for the folder (`invalid marketplace.json: <reason>`). Every row records `manifest.marketplace = { name }`; entries are never expanded as marketplaces; duplicate rel_paths keep the first row.
  2. **Plugin** (`.claude-plugin/plugin.json`): skills are walked in `skills/` or the `plugin.json` `skills` path(s), 3 levels deep.
  3. **Skills package** (`skill_walk::has_skills_package`): a root `SKILL.md` makes the folder one skill (skill rel_path `""`, named after the folder) and nothing else is searched; otherwise `SKILL_CONTAINERS` (`skills`, `skills/.curated`, `skills/.experimental`, `skills/.system`, `.agents/skills`, `.claude/skills`) are walked 3 levels deep and the folder root 1 level. Name = folder name, version `0.0.0`.

  `is_plugin_dir_root` differs from `is_plugin_root` only in skipping the one-level root search (`has_contained_skills`), so a registered dir holding single-skill clones (`<dir>/<repo>/SKILL.md`) lists each clone instead of collapsing into one plugin.
- **Skill walker.** `skill_walk::walk(root, container, depth)` returns sorted rel paths of dirs holding `SKILL.md`: a skill dir's subdirs are not searched (shallower shadows deeper), dirs named `.*` or `node_modules` are skipped (containers are entered explicitly), and symlink cycles are not followed. Skills reached through a symlink resolving outside the plugin root are listed with error `path escapes plugin root`. A skill's name is its folder name. Duplicate names within one plugin: among error-free skills, the first by rel_path wins and the rest get `duplicate skill name`. `SKILL.md` frontmatter is parsed as YAML (`name` and `description` required strings, descriptions whitespace-collapsed, `invalid frontmatter YAML: <reason>`).
- **Git clones.** A git install clones into `<dir>/<repo name>` and rescans; every row the clone folder produced (its own rel_path, `<root>/…`, and `<root>#…`) is stamped `source = git`, the clone's url/ref/commit, and `git_root = <root>`. No row → the clone is removed and the install fails (`no plugin, skills, or marketplace found in this repository`). `plugin_installs.plugin_ids` lists all produced rows (`plugin_id` = first by rel_path, kept for compatibility; the API returns `pluginIds`). Update of any row with a `git_root` pulls `<dir>/<git_root>` once, stops the MCP servers of every sibling row, and re-stamps them all; vanished entries become `missing`, new ones start disabled. Update of an `external` row → 409.
- **External rows.** Status `external` (migration 033). Enabling → 409 `external plugins cannot be enabled`; they are not assignable (only `enabled` + `ok`), cannot be tested (`plugin is not ok`), and have no skills or MCP servers.
- **Skill switches.** `plugins.disabled_skills TEXT[]` holds skill names switched off via `PUT /api/plugins/{id}/skills/{name}` `{ enabled }` (unknown plugin or skill → 404). A scan that leaves the row `ok` drops names no longer in the manifest. Plugin responses carry `skills[].enabled`; the toggle refreshes `SkillCatalog` without them (`PluginSkillSet::from_manifest` skips disabled names), so `skills_for` and `get` omit them and they vanish from the skill index, `skill_list`, and `skill_load` at once, including for running runs.
- **Catalog.** After any mutation the in-memory `SkillCatalog` is refreshed (serialized) from enabled `ok` plugins, minus their disabled skills. A failed refresh is logged; the next one catches up.
- **Token snapshot.** When a run starts, the worker reads the agent's assigned plugins that are enabled and `ok` and stores those ids in the run's MCP token scope. `skill_list` / `skill_load` serve built-in skills plus the catalog entries of that snapshot, so plugins assigned mid-run are not picked up until the next run. Disabling a plugin removes it from the catalog at once, including for in-flight runs.
- **Assignment.** `PUT /api/agents/{id}/plugins` replaces the set and rejects any id that is not enabled + `ok`. Preset `default_plugins` (names) are assigned on agent create, skipping unavailable ones.
- **OpenCode.** Each OpenCode run gets its own `opencode serve` process with a per-run `OPENCODE_CONFIG` pointing at the gateway; the token stays in that process's env. The process is its own session and the group is stopped when the run ends.
- **Capabilities.** `parse_plugin` runs one `CapabilityParser` (`KEY`, `parse(root, plugin_json, layout)`) per capability. A parser returns `CapabilityOutcome::Absent`, `Supported(output)`, `Unsupported(reason)` (recorded in the manifest's `unsupported` as `{ key, reason }`), or `Invalid(reason)` (the whole plugin is `invalid`). `McpServersCapability` reads `.mcp.json`, else the inline `plugin.json` `mcpServers` object (`.mcp.json` wins), into `McpServerEntry { name, transport: Stdio | Http | Unsupported { kind }, error }` with `${…}` placeholders kept verbatim. An unreadable/non-JSON `.mcp.json` or a non-object `mcpServers` makes the plugin invalid; a malformed single entry becomes `Unsupported { kind: "unknown" }` with `error` set. Older stored manifests still deserialize until the startup rescan rewrites them. The plugins API keeps `mcpServers: [{ name, kind }]` and `unsupported: [key]` — commands, env, URLs, and headers are never exposed.

### Adding a plugin capability

1. A `CapabilityParser` in `plugins/capability.rs`, added to the explicit `ctx.parse::<…>()` call list in `parse_plugin` (`plugins/manifest.rs`), and a `PluginManifest` field.
2. Optionally a `ToolSource` that serves it at run time, scoped by `scope.plugin_ids`.
3. Expose it in the plugin response DTO in `api/plugins.rs` (no secrets), `web/src/lib/schemas/plugin.ts`, and render it in `web/src/features/plugins/PluginCard.tsx`.

## Governed knowledge (M06)

Knowledge keeps lifecycle state separate from content. An edit inserts an immutable revision and advances `current_revision_id`; approving (or editing an approved item) activates that revision immediately. Approve, edit, reject, supersede, stale, and expire operations require an optimistic `expectedVersion`.

```text
api/knowledge.rs                          authenticated reads; admin + CSRF lifecycle writes
api/knowledge_compaction.rs               compaction status, Compact now, Retry, Cancel
api/settings.rs                           GET/PUT compaction agent (PUT is admin-only)
domain/knowledge.rs                       types, scope and content validation, risk classification
domain/knowledge_compaction.rs            batch status/trigger, scheduling and byte-budget helpers
services/knowledge_service.rs             revision/lifecycle invariants, compaction inserts, similar items
services/knowledge_compaction_service.rs  queue, batches, drain cycles, completion, failure reconcile
services/workspace_settings_service.rs    compaction agent setting (read-only connectors only)
knowledge/fts.rs                          OR-joined tsquery builder (unaccent, simple config)
knowledge/retrieval.rs                    eligibility CTE, include-all when it fits, else ts_rank_cd top-k
knowledge/compaction_context.rs           bounded `.agent/context.md` for a compaction run
knowledge/candidates.rs                   lenient parse + strict validation of `knowledgeCandidates`
knowledge/policy.rs                       fail-closed approval policy
services/context_budget.rs                ByteTokenCounter, untrusted delimiters, usage snapshots
workers/knowledge_compaction_scheduler.rs reconcile finished runs, scheduled and drain cycles
workers/job_worker/compaction.rs          executes `compact_knowledge` runs
```

**Retrieval.** Knowledge is no longer injected into `.agent/context.md`; agents pull it on demand with the `knowledge_search` MCP tool (approved items only). The query is the agent's search text, normalized and OR-joined into a `to_tsquery('simple', …)`. Relational eligibility (approved, active, unexpired, unsuperseded, confidence, board/agent scope) is materialized first. Items are ranked by `ts_rank_cd`. Zero matches is not an error. The result is rendered as untrusted data, and every returned exact revision is logged once per run in `knowledge_usage_logs`.

**Compaction.** A trigger queues a ticket when it enters Done and removes its unbatched row when it leaves Done. The scheduler starts a drain cycle every `knowledge.compaction.interval_secs` (or on Compact now) when an enabled compaction agent is configured. Each batch is one `compact_knowledge` run of that agent: read-only tools, a scratch directory, no repository, no knowledge retrieval, no ticket side effects. The agent returns `knowledgeCandidates`; invalid candidates (sources outside the batch, board mismatch, unknown types, limits) are dropped with reasons in the batch summary. Valid ones go through the fail-closed policy: workspace scope, supersessions, and high-impact types always need human approval. Success deletes the batch's queue rows and sends no notification. The scheduler's reconcile step fails batches whose run ended without applying a result: tickets return to the queue with `attempts + 1`, and a `knowledge_compaction_failed` notification goes to every user. Cancelled runs release tickets without an attempt or notification. Tickets at `max_attempts` wait for a manual Retry. Operator settings: [Knowledge configuration](operations.md#knowledge-configuration).

**Config env:** `AGENT_DEFAULT_PROVIDER`, `WORKTREES_PATH`, `AGENT_WORKER_COUNT` (see `deploy/docker-compose.yml`). Operator bind-mounts host clones; register in-container paths in Settings → Repositories.

## Web frontend

```text
web/src/
  features/     auth, boards, board, tickets, agents, knowledge, plugins, tools, users
  components/   AppShell, ProtectedRoute, shared UI
  lib/          api.ts (fetch + CSRF), schemas/ (Zod), query-client
  styles/       tokens.css (design tokens)
```

- **Routing:** React Router; `/login` public, everything else behind `ProtectedRoute`.
- **Data:** TanStack Query hooks per feature (`useTickets`, `useAgents`, …).
- **API client:** `lib/api.ts` — `credentials: 'include'`, CSRF header on writes.
- **Board:** fixed columns in `features/board/columns.ts`; dnd-kit for drag-and-drop.
- **Plugins:** Settings → Plugins manages dirs, rescan, git installs, and enablement. The agent form has a Plugins checkbox list (enabled `ok` plugins only); assigned plugins that became unavailable are shown disabled and dropped on save.
- **Tools:** `/tools` (admin) has **Backup** and **Connectors** tabs; the tab is kept in the URL (`/tools?tab=connectors`). Connectors shows one card per connector except `mock` from `GET /api/tools/connectors`; Run check re-probes one card, and Test connection picks an agent on that connector, starts a check, and polls it until passed/failed.
- **Knowledge:** `/knowledge` has Pending, Approved, Rejected, and Stale views with provenance and lifecycle controls. Expanded Agent Run details load the immutable **Knowledge Used** audit.
- **Forms:** React Hook Form + Zod schemas in `lib/schemas/`.

Visual design tokens and palette: `docs/web/DESIGN.md`.

## CLI

`cli/` — operator CLI: migrate, health, bootstrap, `server start`, `web start`, `connector …` (install/doctor/list from the `connectors` descriptors). Shares TOML config with the server. `coppice web start` serves the built SPA and proxies `/api` to the API.

## Config & artifacts

- Host/release: `config.toml` (see root `config.example.toml`); Docker Compose: `deploy/config/config.toml` (see `deploy/config/config.example.toml`), bind-mounted as `COPPICE_CONFIG`
- Attachments: filesystem under `storage.artifacts_dir` (compose volume `artifact_data`)
- Static SPA (release): `coppice web start` via `[web].static_dir`; desktop mode serves it from the API origin (below)

## Desktop mode (M11)

`coppice-server desktop --data-dir <D> --resources <R>` ([design](superpowers/specs/2026-10-04-desktop-release-design.md)) is the single child process of the Electron shell (`desktop/main.mjs`).

- **Data dir `D`** (Electron `userData`): `config.toml` is generated once; later connector on/off changes patch that file in place and keep comments and other keys. `secrets/` is generated once and never overwritten. Postgres cluster in `pg/data`, storage dirs, `logs/`.
- **Resources `R`:** `bin/coppice-server`, a pinned Postgres 16 bundle (`desktop/postgres.lock.json`), `web/`, agent templates. The server exports `COPPICE_PG_BIN_DIR` / `COPPICE_PG_LIB_DIR` (`R/postgres/bin`, `R/postgres/lib`) so Tools → Backup runs the bundled `pg_dump` / `psql` by absolute path (with `LD_LIBRARY_PATH` on Linux for that command only); `PATH` is left alone, so agents keep the user's own `psql`.
- **Postgres:** `initdb` on first run, major-version check, stale `postmaster.pid` cleanup, TCP on `127.0.0.1` and a free port only.
- **Server:** binds `127.0.0.1` on a free port, forces `auth.desktop_mode`, serves `R/web` with SPA fallback on the API origin (`static_web.rs`), then prints `COPPICE_READY url=…` once.
- **Shutdown** on SIGTERM, SIGINT, or stdin EOF (Electron died), bounded to Electron's 15 s window: 5 s drain (including a 3 s SIGTERM grace for agent process groups, then SIGKILL), `pg_ctl stop` fast (5 s) then immediate (3 s), 1 s for the runtime (14 s worst case). Electron waits that 15 s before SIGKILL because agent sessions are not in the server's process group. The non-desktop server listens for SIGTERM and SIGINT and runs the same agent-tree stop. A crash leaves `agent-processes.json`; the next launch reaps those pids only when the start token and command still match.
- **Electron** resolves the login-shell `PATH` for agent CLIs, writes child output to `logs/server.log` (10 MB × 3), shows a failure window if the ready line does not arrive, and polls GitHub `releases/latest` for the update banner.

## Milestone evolution

Each milestone adds modules/tables/endpoints documented in `docs/milestones/M0N-*.md`. Through M06 the system includes boards, repositories, tickets, collaboration workflow, live agent runs, governed long-term knowledge, and bounded/auditable context assembly. M07–M09 add git/PR actions with forge secrets, managed connectors, and Agent Chat. M10 adds plugins and the MCP gateway; M11 the desktop app and release pipeline. **Next:** M12 beta release (site + installable desktop), then M13 security & sandbox, then M14 role-owner agents.
