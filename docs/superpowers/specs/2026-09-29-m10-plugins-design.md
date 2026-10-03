# M10 Plugins Design

**Status:** Approved — Part 1 (steps 1–4), Part 2a, the foundations, and [Part 2b](2026-10-02-m10-part2b-plugin-mcp-design.md) (steps 6–7) implemented. Remaining: live verification of the `claude-code`, `codex`, and `kilo-code` wiring (manual acceptance)  
**Date:** 2026-09-29  
**Owner/reviewer:** Technical Lead  
**Milestone:** [M10 — Plugins](../../milestones/M10-plugins.md)

## Decision summary

Coppice keeps wrapping vendor CLIs (`claude-code`, `cursor`, `codex`, `opencode`, `kilo-code`, `mock`) and becomes the **single MCP gateway** for every run. Each run's connector is configured with exactly one MCP server, `coppice`, served by the Coppice server at `/mcp` and authenticated by a per-run token. Behind that endpoint:

- **Core tools** expose Coppice itself (tickets, comments, agents, knowledge, results) as thin adapters over existing services.
- **Skill tools** (`skill_list`, `skill_load`) serve skills from enabled plugins; the slim context carries only a skill index.
- **Plugin tools** are proxied from plugin MCP servers (stdio children or remote HTTP) under namespaced names.

Plugins use the existing **Claude Code / Cursor plugin format** (`.claude-plugin/plugin.json`, `skills/`, `.mcp.json`), so third-party plugins work unchanged. Admins register one or more **plugin directories** and can install from a **git URL** into one of them. Installed plugins are workspace-wide and start disabled; each agent selects whole plugins.

Every connector moves to **one tool-first path**: a slim `context.md` plus `result_submit`. Parsing a final JSON object stays only as a safety net. The legacy fat context is deleted once all connectors pass.

## Goals

- Cut per-run context substantially (target ≥50% for `full` on fixture tickets) by moving data behind tools and the result contract into a validated tool schema.
- One enforcement and audit point for all agent tool use, so M11 policy lands without rework.
- Plugin UX: pick plugin directories, install from git, enable, assign to agents, see what each run used.
- Work identically in Docker Compose, desktop (host process), and future cloud.
- Keep workflow semantics exactly as today (M05 result contract, M06 inbox approval, M09 chat write-denial).

## Non-goals

- Coppice-owned agent loop calling LLM APIs directly.
- Plugin storage (dropped from M10; revisit with a concrete use case).
- Plugin `commands/`, `agents/`, `hooks/` — detected and listed as unsupported, never executed.
- Marketplaces, plugin signing, remote catalogs.
- Personal access tokens / external agents using Coppice MCP (token layer is designed to allow it later).
- Sandboxing plugin processes, per-tool capability grants, secret scoping (M11).
- Signals and observation tools (M12).

## Current state (baseline)

- `job_worker` writes `.agent/context.md` (from `context_builder.rs`, ~1.6k lines, five profiles: `full`, `human_agent`, `human_chat`, `conversation`, `knowledge_compaction`) plus `.agent/ticket.json`, `comments.json`, `runs.json`.
- Run prompt (`COPPICE_RUN_PROMPT`): "Read .agent/context.md … reply with ONLY a single JSON object matching the done or blocked contract".
- Result is parsed from the connector's final output into `AgentRunResult` and applied by `result_contract::apply_agent_result` / `apply_consultation_result`, and chat/compaction equivalents.
- Read-only enforcement is per connector and uneven (`READ_ONLY_CAPABLE_CONNECTORS = mock, claude-code, cursor`).
- Connectors spawn CLIs inside the server process environment (server container in Docker; host in desktop). OpenCode runs through a shared `opencode serve` process.

The implementation plan's first task records baseline `context.md` sizes per profile on fixture tickets (via `context_budget`).

## Architecture

```text
 connector CLI (claude / cursor / codex / opencode / kilo / mock)
        │  one MCP server entry "coppice"  (streamable HTTP, Authorization: Bearer <run token>)
        ▼
 ┌──────────────────────── Coppice server ────────────────────────┐
 │ /mcp ── RunTokenAuth ── ToolRouter ──┬─ core tools → services   │
 │                                      ├─ skill tools → SkillCatalog
 │                                      └─ plugin tools → McpProxy ──► plugin MCP servers
 │ PluginRegistry ◄── plugin_dirs scan + git install               │
 │ ToolCallLog (run_tool_calls)                                    │
 └─────────────────────────────────────────────────────────────────┘
```

| Unit | Responsibility | Depends on |
|------|----------------|------------|
| `plugins::registry` | Scan plugin dirs, parse manifests, persist `plugins`, resolve shadowing, enable/disable | `plugin_dirs`, filesystem |
| `plugins::manifest` | Parse Claude Code / Cursor plugin format and skills-only folders | — |
| `plugins::git_install` | Background shallow clone / update into a plugin dir, record commit | `git` on PATH, host credentials |
| `plugins::skills` (`SkillCatalog`) | Skill index for a run; load skill body + folder path | registry, `agent_plugins` |
| `mcp::server` | Stateless Streamable HTTP MCP endpoint on the existing Axum app | Axum |
| `mcp::token` | Mint / verify / revoke run tokens | `run_tool_tokens` |
| `mcp::router` | Resolve the tool set for a token, dispatch, log, enforce limits | all tool sources |
| `mcp::core_tools` | Core tool handlers → `ticket_service`, `comment_service`, `knowledge_service`, `result_contract`, chat actions | existing services |
| `mcp::proxy` | Lifecycle and multiplexing of plugin MCP servers; tool namespacing | `rmcp` client |
| `providers::*` | Per-connector MCP wiring for the run | run token, `mcp.base_url` |
| `cli mcp-bridge` | stdio ↔ HTTP bridge for stdio-only connectors | — |

Rules:

- **Server owns state.** Core tools call services; no workflow logic in tool handlers or the SPA.
- **Everything goes through the router.** Plugin tools are never wired directly into a connector.
- **Protocol implementation:** the gateway is a small **stateless** MCP Streamable HTTP server written directly on Axum (POST JSON-RPC → `application/json` response; `GET` returns 405, which the MCP spec allows). Methods: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`. This keeps auth and scoping in plain Axum extractors. The official Rust SDK `rmcp` is used as the **client** for plugin MCP servers (Part 2).

## Plugin format and discovery

### Accepted layouts

1. **Plugin:** a directory containing `.claude-plugin/plugin.json` (name, version, description, optional author), with optional `skills/<name>/SKILL.md`, `.mcp.json`, `commands/`, `agents/`, `hooks/`.
2. **Skills-only folder:** a directory with one or more `<name>/SKILL.md` (or a `skills/` subdirectory) and no `plugin.json`. Treated as a plugin named after the folder, version `0.0.0`.

A plugin directory is scanned at depth 0 (the directory itself is a plugin) and depth 1 (each child is a plugin). Deeper nesting is ignored.

### Skills

- `SKILL.md` frontmatter `name` and `description` are required; missing either marks that skill invalid (plugin stays usable).
- Skill identity for agents: `<plugin>:<skill>` (e.g. `superpowers:brainstorming`); the bundled `coppice` plugin's skills may be referenced unqualified.

### MCP servers (`.mcp.json`)

- Supported entries: stdio (`command`, `args`, `env`) and remote (`type: http` / `url`, `headers`). SSE-only remotes are listed as unsupported.
- Placeholder substitution: `${CLAUDE_PLUGIN_ROOT}` → plugin path; `${VAR}` → plugin setting `VAR`, else server environment `VAR`, else the server fails to start with a clear error. *Amended 2026-10-02:* the server-environment fallback excludes `COPPICE_*`, `DATABASE_URL`, and `SECRETS_MASTER_KEY`; `${VAR:-default}` uses `default` when both are absent; the error is `missing setting "VAR"`; stdio children get a minimal environment (`PATH`, `HOME`, `LANG`, `TMPDIR` plus the entry's `env`) — see [Part 2b design](2026-10-02-m10-part2b-plugin-mcp-design.md).

### Unsupported parts

`commands/`, `agents/`, `hooks/` are detected, listed on the plugin card as "not supported yet", and never executed.

### Shadowing

Plugin names are unique workspace-wide. If two dirs contain the same name, the dir earlier in admin-defined order wins; others are recorded with status `shadowed`.

## Plugin lifecycle

- **Plugin dirs:** admin adds by path (desktop: Electron Browse), orders, removes. A default dir (`[plugins] dir`, default `./data/plugins`; Docker `/data/plugins` on the `plugin_data` volume) is created on first start and cannot be removed.
- **Scan:** on server start, on dir add, and on explicit Rescan. Scan upserts `plugins` by (dir, relative path); plugins no longer on disk become `missing` (agent assignments kept).
- **Git install:** `POST /api/plugins/install { gitUrl, ref?, pluginDirId }` → background job: shallow clone into `<dir>/<repo-name>`, record `git_url`, `git_ref`, `git_commit`, then scan. Update pulls and re-records the commit. Uses host git credentials (same posture as repositories). **This is an explicit exception** to the "no server-side git clone" rule, which continues to apply to repositories.
- **Enablement:** installed plugins start `enabled = false`. Enabling is admin-only. Enabling a plugin with stdio MCP servers shows a warning that they run with server privileges until M11.
- **Agent assignment:** `agent_plugins` (whole plugins only). Presets list default plugin names; applied when the plugin exists and is enabled. The bundled `coppice` plugin is always on and not listed in the picker.
- **Plugin settings:** per-plugin env/header values, stored encrypted via the M07 `secrets` store; write-only in UI/API.

## Built-in `coppice` plugin

Shipped with the server (read-only, not in any user plugin dir). Contains platform skills that replace long prose in `context_builder.rs`:

| Skill | Content moved from context today |
|-------|----------------------------------|
| `coppice-collaboration` | `mentionAgents` / `agentRequests` / `assignTo` semantics, one-hop consultation rules |
| `coppice-splitting` | `splitTickets` / `continued` long-running guidance |
| `coppice-pm-refinement` | PM refinement rules |
| `coppice-tech-lead-review` | Ready refinement + In Review rules |
| `coppice-qc-verification` | QC verification-only rules |
| `coppice-git` | Worktree / commit rules beyond the one-liners |

Must-never-miss rules stay as one line each in the slim context. When a role/stage skill applies, the context names it ("Load `coppice-qc-verification` before starting").

## Core tools

Tool names are short; clients present them as `coppice.<tool>` / `mcp__coppice__<tool>`. Exact JSON schemas are part of the implementation plan.

| Tool | Behavior |
|------|----------|
| `ticket_get` | Current ticket by default; `ticketId` for another ticket on the same board. Title, description, acceptance criteria, status/substatus, assignee, repo, branch |
| `ticket_comments` | Paginated (`limit`, `before`), newest first |
| `ticket_runs` | Past run summaries for a ticket |
| `ticket_search` | Text/status search within the board |
| `board_agents` | Enabled agents: key, name, role |
| `knowledge_search` | Approved, in-scope, unexpired knowledge via existing FTS; each returned revision is logged once per run as Knowledge Used |
| `skill_list` | Skills available to this run (name, description) |
| `skill_load` | Skill body + absolute folder path; logged as Skills Used |
| `comment_post` | Markdown note on the current ticket, authored by the agent; max 5 per run |
| `result_submit` | Final result for the run; schema depends on profile |

### Profile → tool matrix

| Tool | `full` | `human_agent` | `human_chat` | `conversation` | `knowledge_compaction` |
|------|:-:|:-:|:-:|:-:|:-:|
| `ticket_get`, `ticket_comments`, `ticket_runs` | ✓ | ✓ | ✓ | ✓ (board) | ✓ (batch tickets only) |
| `ticket_search`, `board_agents` | ✓ | ✓ | ✓ | ✓ | — |
| `knowledge_search` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `skill_list`, `skill_load` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `comment_post` | ✓ | ✓ | — | — | — |
| `result_submit` | done/blocked/continued | done/blocked | reply (done/blocked) | reply (done/blocked) | done + `knowledgeCandidates` |
| Plugin tools | all | all | `readOnlyHint` only | `readOnlyHint` only | — |

Change from M09: `knowledge_search` is allowed in `conversation`. It returns only approved knowledge on demand and does not bypass the Knowledge Inbox; M09's concern was pre-injecting retrieval into chat context.

Not tools: chat actions (`create_ticket`, `create_knowledge`, `cutoff`) remain **human-triggered** SPA/API actions as in M09. Knowledge creation stays with the compaction agent (`knowledgeCandidates` in its result); regular runs get no knowledge-proposal tool.

### `result_submit`

- Input: one schema for every profile — the existing `AgentRunResult` JSON (`status: done | blocked | continued` and its fields). Profiles restrict which statuses/fields are meaningful (table above).
- The server validates with the existing pure functions (`result_contract::apply_agent_result` / `apply_consultation_result`) plus profile rules, with no side effects, and returns structural errors as a tool error so the agent can resubmit.
- Targets the workflow would ignore (unknown / disabled / self / over-limit agents in `assignTo`, `mentionAgents`, `agentRequests`) are **warnings**, not errors — M05 semantics are unchanged; warnings let the agent fix them.
- The latest valid submission is stored on the run (`agent_runs.submitted_result`). It is **applied when the run finishes**, through the same code path as today. Stop/cancel discards it.
- Idempotent: resubmitting replaces the stored submission.
- Fallback: if no valid submission exists at finish, the final-output JSON is parsed as today. If neither yields a valid result, the run fails with the existing "no result" error.

## Slim context

`context.md` contains only:

1. Agent identity (name, role, system prompt / soul).
2. Task: job type, human request (if any), ticket headline (title, status, substatus, assignee).
3. Worktree path and one-line git rules.
4. Skill index (`name — description`), with any required skill called out.
5. Platform one-liners, including: fetch details with `ticket_get` / `ticket_comments`; search knowledge with `knowledge_search`; finish by calling `result_submit`.

Removed: embedded contract JSON, collaboration prose, role-rule blocks, `.agent/*.json` files, pre-injected knowledge, full thread excerpts.

New run prompt: "Read .agent/context.md, complete the task, and call the `result_submit` tool when finished."

`context_budget` keeps reporting sizes; the plan asserts the reduction target on fixture tickets.

## Run tokens

- Minted at run start (and per chat turn): 256-bit random, only the SHA-256 hash stored in `run_tool_tokens` with `run_id`, `agent_id`, `ticket_id` / `chat_session_id`, `board_id`, `context_profile`, `plugin_ids` snapshot, `expires_at`, `revoked_at`.
- Revoked on finish / stop / failure; hard expiry = run timeout + margin.
- Sent as `Authorization: Bearer`, never in URLs or logs. The token reaches the connector through per-process environment (`COPPICE_MCP_URL`, `COPPICE_MCP_TOKEN`) or a per-run config file in the run's artifacts dir — never the worktree, a registered repo checkout, or the user's global CLI config.
- `subject_kind` column (`run` now; `personal` reserved) keeps personal access tokens possible later without schema rework.

## Gateway endpoint

- `POST/GET /mcp` streamable HTTP on the existing server; not behind session/CSRF middleware (token auth instead).
- Base URL from config `mcp.base_url`, default `http://127.0.0.1:<server.port>/mcp` — correct for Docker (CLIs run in the server container), desktop (host, dynamic port), and cloud.
- Tool list per session is computed from the token: profile matrix ∩ agent's enabled plugins ∩ plugin health.
- Limits: per-call timeout 60 s (configurable), output cap with truncation + pagination hint, `comment_post` cap, max concurrent calls per run.

## Connector wiring

Wiring rules (apply to Docker, desktop, and cloud alike):

1. Per-run configuration only: CLI flags, per-process env, or a config file in the run's artifacts dir referenced by flag/env.
2. Never write into the worktree, a registered repo checkout (chat may run in one), or the user's global CLI config (desktop uses the real `$HOME`).
3. Token only via `COPPICE_MCP_TOKEN` env or the per-run file.

**Verification status (plan task 1, `server/examples/mcp_probe.rs`, Cursor Agent CLI `2026.09.28-64d2043`).** Only `cursor` was verified against a live CLI; it is the only connector CLI installed on the dev machine. The other rows remain *unverified — expected mechanism* and must be verified when their CLI is first wired (their plan tasks), applying the decision rules in the last column.

| Connector | Status | Mechanism |
|-----------|--------|-----------|
| `claude-code` | unverified — expected mechanism | `--mcp-config <run file> --strict-mcp-config`; allow `mcp__coppice__*` in `--allowedTools` |
| `codex` | unverified — expected mechanism | `-c mcp_servers.coppice.url=…` + `-c mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"` |
| `cursor` | **verified** | Per-run env only (below); tool-first is viable |
| `kilo-code` | unverified — expected mechanism | Per-process config path env (`KILO_CONFIG` / fork equivalent) pointing at the run file |
| `opencode` | Part 1: returns `mcp_unavailable`. Part 2a: **verified** (OpenCode `1.18.33`) | Shared `opencode serve` cannot carry per-run env. Part 2a replaces it with a **per-run `opencode serve`** (below) |
| `mock` | n/a | Fixture field `toolCalls: [{ tool, args }]` executed over HTTP JSON-RPC against `/mcp` before returning the fixture result |

### Cursor: verified mechanism

The CLI reads MCP servers only from `<workspace>/.cursor/mcp.json` and `$HOME/.cursor/mcp.json` (Node `homedir()`, i.e. the `HOME` env var); `--plugin-dir` with `mcp.json` did **not** register servers (`agent mcp list` empty, model saw no tool). `CURSOR_CONFIG_DIR` alone does **not** relocate `mcp.json`. What works, with nothing written to the workspace or the user's real `~/.cursor` / `~/.config/cursor` config:

Exact setup used in the verification run. `<home dir>` and `<config dir>` are the per-run directories (verification used `/tmp/cur-home3` and `/tmp/cur-cfg3`); the token value is per run (verification used `probe-token`); the port varies (verification used `5099`). Everything else was used verbatim.

`<home dir>/.cursor/mcp.json`:

```json
{"mcpServers":{"coppice":{"url":"http://127.0.0.1:5099/mcp","headers":{"Authorization":"Bearer ${env:COPPICE_MCP_TOKEN}"}}}}
```

`<config dir>/cli-config.json`:

```json
{"version":1,"permissions":{"allow":["Mcp(coppice:*)"],"deny":[]}}
```

Command (working directory: an empty scratch workspace, `/tmp/cur-ws`):

```sh
env HOME=<home dir> XDG_CONFIG_HOME=/home/hungnguyenba/.config CURSOR_CONFIG_DIR=<config dir> COPPICE_MCP_TOKEN=<run token> \
  agent -p --mode ask --trust --output-format stream-json \
  "Call the coppice ping tool with message hello and print the result."
```

`XDG_CONFIG_HOME=/home/hungnguyenba/.config` is the verifying user's real config home (the value `agent` uses when the variable is unset, `$HOME/.config` of the real user); it keeps `auth.json` reachable at `$XDG_CONFIG_HOME/cursor/auth.json`. The connector must pass the real user's config home here, because `HOME` no longer points at it. `CURSOR_CONFIG_DIR` makes permissions come from `<config dir>/cli-config.json` instead of the real `$XDG_CONFIG_HOME/cursor/cli-config.json`.

Results:

- `${env:COPPICE_MCP_TOKEN}` header interpolation works (probe saw `authorized=true`); the client sends `Authorization` on every request.
- Prompt "Call the coppice ping tool with message hello…" returned `pong hello` in `--mode ask` (read-only), for a fresh `HOME`. No `--force`/`--yolo` needed. `--approve-mcps` is **not** needed in `-p` mode with `--trust`.
- **Without** the `Mcp(coppice:*)` allow rule, `-p` denies the call ("rejected at the approval prompt"), also with `--approve-mcps`. The rule must live in the `cli-config.json` that `CURSOR_CONFIG_DIR` points at; an allow rule placed in `$HOME/.cursor/cli-config.json` is ignored when `XDG_CONFIG_HOME` is set.
- Tool-name form is `<server>-<tool>` (`coppice-ping`, stream-json `mcpToolCall.args.name`), not `mcp__coppice__ping`. The model discovers tools via a built-in MCP lookup (server `coppice`, tool `ping`), so the prompt need not list them.
- Client `initialize` sends `protocolVersion: "2025-11-25"`, `capabilities.elicitation.form`; the gateway answers `2025-06-18` and the client accepts it. It opens `GET /mcp` after `notifications/initialized` (expects SSE); a `405` is tolerated. It re-initializes on several connections per run, so the gateway must be stateless per request (no `Mcp-Session-Id` requirement).
- Side effects: overriding `HOME` also changes `HOME` for the agent's own shell commands (git identity, ssh keys, `gh` auth are not inherited). The connector must either keep such work in Coppice-owned steps (branch/commit/push are, per M07) or forward the needed git env (`GIT_CONFIG_GLOBAL`, `GIT_SSH_COMMAND`, …) explicitly. The CLI also writes its own state (`chats`, `statsig-cache.json`, and a default `cli-config.json` if absent) under `$XDG_CONFIG_HOME/cursor` when `CURSOR_CONFIG_DIR` is not set — always set it.
- Decision: `cursor` **is tool-first** using the per-run `HOME` mechanism above.

### OpenCode: per-run `opencode serve` (Part 2a)

Rejected alternatives: per-run `opencode run` (loses the session/event API the OpenCode live console depends on) and a shared server with per-run MCP servers registered through its API (MCP tools are global to the server, so concurrent runs would see each other's tools and tokens).

- Each run (and each chat turn that has no live session process) spawns its own `opencode serve` on a free loopback port with `OPENCODE_CONFIG=<run dir>/opencode.json`. The file registers one remote MCP server `coppice` at `mcp.base_url` with header `Authorization: Bearer {env:COPPICE_MCP_TOKEN}`; the token is passed only in the process environment.
- The existing `opencode_client` / `opencode_events` code talks to that process unchanged; only process ownership moves from the global `opencode_serve` singleton to the run.
- The process is killed on finish, stop, cancel, and failure (same paths that revoke the token), with a `Drop` backstop. Resume needs no per-session directory: OpenCode keeps sessions in its shared data dir, so a new per-run process continues an existing session id.
**Verification (plan 2a task 1, OpenCode `1.18.33` installed with `coppice connector install opencode` in the default Compose server container, `server/examples/mcp_probe.rs` on the host at `HOST=0.0.0.0`, free model `opencode/big-pickle`).** Per-run file, used verbatim except the URL:

```json
{"$schema":"https://opencode.ai/config.json","mcp":{"coppice":{"type":"remote","url":"http://172.19.0.1:5099/mcp","enabled":true,"headers":{"Authorization":"Bearer {env:COPPICE_MCP_TOKEN}"}}}}
```

Command: `env OPENCODE_CONFIG=<run dir>/opencode.json COPPICE_MCP_TOKEN=<token> opencode serve --hostname 127.0.0.1 --port <free port>`, driven with `opencode run --attach http://127.0.0.1:<port> -m opencode/big-pickle --format json "Call the coppice ping tool …"`.

- `{env:COPPICE_MCP_TOKEN}` header interpolation works: every probe request (`initialize`, `notifications/initialized`, `GET /mcp`, `tools/list`, `tools/call`) had `authorized=true`; `GET /mcp?directory=<ws>` on the serve reports `{"coppice":{"status":"connected"}}`. Result `pong hello`. Client `protocolVersion` `2025-11-25`, accepts the gateway's answer.
- Tool-name form in the event stream: `coppice_ping` (`<server>_<tool>`).
- `OPENCODE_CONFIG` merges with the global config: the managed install's models/auth still apply.
- Concurrency: two and three `opencode serve` processes on different ports with separate config files run at the same time; each run called only its own gateway (`pong hello` / `pong world`).
- Resume: a session created on process B, B killed, prompted with `--session <id>` on process A in the same directory — accepted and continued (sessions live in the shared `$HOME/.local/share/opencode/opencode.db`).
- No permission prompt for MCP tools in serve mode; nothing written into the workspace.
- Startup quirk: an HTTP request sent while the process is still booting can be accepted and never answered. Health checks must use a short per-request timeout and retry (the existing `/doc` probe uses 2 s).

If the connector cannot reach the gateway, the run fails with `mcp_unavailable` — no silent fallback to the fat context.

Chat write-denial (M09) is unchanged for connector-native tools; Coppice tools follow the profile matrix.

## Plugin MCP proxy

- One managed instance per enabled plugin MCP server, **shared across runs**, started lazily on first `tools/list` that needs it.
- Concurrent requests are multiplexed over the single client connection (JSON-RPC ids).
- Crash → restart with exponential backoff; repeated failure marks the server `unhealthy` and hides its tools until the next successful health check.
- Idle shutdown after a configurable period.
- Tool list cached; refreshed on `notifications/tools/list_changed` and on restart.
- Exposed names: `<plugin>__<tool>` (sanitized to MCP name rules); descriptions and `annotations` passed through.
- Documented limitation: shared instances share state across runs; per-run instances can be added later if needed.

Structure (amended 2026-10-01, on the [foundations](2026-10-01-connector-and-tool-source-foundations-design.md)):

- `PluginMcpSource` is the third `ToolSource` in the gateway `ToolRegistry`. It lists tools only for plugins in the token's `plugin_ids` snapshot that are still enabled and `ok`, and only for servers that are healthy. The router's chat read-only filter (`readOnlyHint`) applies to it like every source; calls are logged with `source = plugin` and `plugin_id`.
- `McpServerPool` owns the shared instances (lazy start, multiplexing, restart with backoff, unhealthy, idle shutdown, tool-list cache). It starts servers from the full `McpServerEntry` kept by the `mcpServers` capability parser, substituting placeholders at start.
- Transports sit behind a small `McpTransport` trait built on the `rmcp` client: stdio and streamable HTTP now; a new transport (e.g. SSE) is one implementation.
- Live console: each structured console parser splits gateway tool names with its connector descriptor's `ToolNameStyle`, so Coppice and plugin tools render the same way in every console.
- Enabling a plugin that has stdio MCP servers shows the server-privileges warning (until M11).

## Data model

```text
plugin_dirs       (id, path, position, is_default, created_at)
plugins           (id, plugin_dir_id, rel_path, name, version, description, source [local|git|builtin],
                   git_url, git_ref, git_commit, manifest jsonb, status [ok|invalid|missing|shadowed],
                   error, enabled, created_at, updated_at)
plugin_settings   (plugin_id, key, secret_id)
agent_plugins     (agent_id, plugin_id)
plugin_installs   (id, plugin_dir_id, kind [install|update], git_url, git_ref, plugin_id,
                   status [running|succeeded|failed], error, created_at, finished_at)
agent_presets     + default_plugins text[]
run_tool_tokens   (id, token_hash, subject_kind, run_id, agent_id, ticket_id, chat_session_id, board_id,
                   context_profile, plugin_ids, expires_at, revoked_at, created_at)
run_tool_calls    (id, run_id, tool, source [core|skill|plugin], plugin_id, args_summary, status [ok|error|denied|timeout],
                   error, duration_ms, created_at)
agent_runs        + submitted_result jsonb
```

Skills Used is derived from `run_tool_calls` where `tool = skill_load`; Knowledge Used keeps its existing M06 table.

## API

```text
GET    /api/plugin-dirs
POST   /api/plugin-dirs
PATCH  /api/plugin-dirs/:id            # reorder
DELETE /api/plugin-dirs/:id
POST   /api/plugins/rescan
POST   /api/plugins/install            # { gitUrl, ref?, pluginDirId } → job id
GET    /api/plugin-installs/:id        # install/update job status
POST   /api/plugins/:id/update         # git pull
GET    /api/plugins
GET    /api/plugins/:id                # skills, MCP servers, tools, unsupported parts, status
PATCH  /api/plugins/:id                # enable / disable
PUT    /api/plugins/:id/settings       # write-only values
POST   /api/plugins/:id/test           # start MCP servers, list tools
GET    /api/agents/:id/plugins
PUT    /api/agents/:id/plugins
GET    /api/agent-runs/:id/tool-calls
POST   /mcp                            # run token auth
```

Admin-only for all plugin and plugin-dir mutations; CSRF applies to all `/api` mutations.

## UI

- **Settings → Plugins:** plugin dirs (add / Browse / reorder / remove / Rescan), Install from git, plugin cards (name, version, source + commit, skill / MCP counts, unsupported parts, status, error, enable toggle, Update, Test), plugin detail (skills with descriptions, tools, settings).
- **Agent form:** Plugins multi-select (enabled plugins only).
- **Agent Run detail:** Tools & Skills tab (call log, Skills Used, Knowledge Used).
- **Live console:** tool calls inline, reusing `web/src/opencode-session/tools/` renderers.

## Error handling

- Tool-level failures → MCP tool result with `isError: true` and an actionable message (e.g. `ticket not on this board`, `result invalid: assignTo references unknown agent "foo"`).
- Auth failures → HTTP 401; revoked/expired tokens included.
- Denied (tool not in the token's set) → tool error `denied`, logged.
- Plugin server unavailable → tool error `plugin "<name>" unavailable`; run continues.
- Timeouts → tool error `timeout`, logged.
- Git install failures → job status `failed` with the git error shown on the card.
- Invalid manifest → plugin `invalid` with error; skills-level errors mark only that skill.

## Testing

### Unit

- Manifest parsing: plugin layout, skills-only layout, a fixture shaped like `superpowers`, invalid frontmatter, unsupported parts detection.
- Shadowing and missing-plugin handling on rescan.
- Placeholder substitution (`${CLAUDE_PLUGIN_ROOT}`, settings, env, missing var).
- Token mint/verify/revoke/expiry; wrong board/ticket rejected.
- Profile → tool matrix; `readOnlyHint` filtering for chat profiles.
- `result_submit` validate-only paths for every profile, including field-level errors and idempotency.
- Tool name sanitization and namespacing.

### Integration (embedded Postgres, mock connector)

- Mock run with `toolCalls` (`ticket_get` → `knowledge_search` → `skill_load` → `result_submit`) over real HTTP MCP produces the same ticket state as the equivalent legacy fixture.
- Fallback: no `result_submit` → final JSON applied.
- Token rejected after run finish.
- `knowledge_search` records Knowledge Used once per revision per run; `skill_load` records Skills Used.
- Chat turn: write plugin tools not listed; `result_submit` reply becomes the agent chat message.
- Stdio fixture MCP server (small Rust test binary) proxied end to end; crash → restart; unhealthy hides tools.
- Plugin dirs, rescan, enable, agent assignment APIs; admin-only and CSRF enforced.
- Context size assertion against recorded baseline.

### Smoke

- `make e2e-smoke-m10`: add a fixture plugin dir, enable plugin, assign to agent, run a mock ticket that uses core tools, a plugin skill, and a plugin MCP tool; verify Tools & Skills tab.
- Existing `make e2e-smoke-m03`, `e2e-smoke-m06`, `e2e-smoke-m06-knowledge`, `e2e-smoke-m09` still pass.

### Manual acceptance

Each real connector completes a ticket tool-first (tools called, `result_submit` used) in the default Compose stack with managed connectors (M08).

## Delivery order

Plans: [Part 1 — tool-first harness](../plans/2026-09-29-m10-part1-tool-first-harness.md) covers steps 1–4 (merged). Part 2 is split into two plans:

- **Part 2a — plugins and plugin skills:** OpenCode per-run `opencode serve` (verification first), then step 5: migration (`plugin_dirs`, `plugins`, `agent_plugins`, `run_tool_tokens.plugin_ids` — the snapshot column was not added in Part 1), manifest parsing and scan (shadowing, missing, rescan, unsupported parts), git install/update job, enable/disable, agent assignment and presets, `SkillCatalog` serving built-in + snapshot plugin skills as `<plugin>:<skill>`, Settings → Plugins and the agent-form picker, integration tests with a mock run calling `skill_load` on a plugin skill.
- **Foundations (between 2a and 2b):** [Connector and tool-source foundations](2026-10-01-connector-and-tool-source-foundations-design.md) — connector descriptors, MCP wiring renderers, shared CLI runner, gateway `ToolSource` registry, plugin capability parsers. No behavior change. Part 2b depends on it.
- **Part 2b — plugin MCP and observability:** built on the foundations (see "Plugin MCP proxy"); detailed in [Part 2b design](2026-10-02-m10-part2b-plugin-mcp-design.md). Steps 6–7: `plugin_settings` (encrypted), `mcp::proxy` over `rmcp` (stdio + HTTP, namespacing, shared instances, restart/backoff, unhealthy, idle shutdown, `readOnlyHint` filtering for chat profiles), Test button, Tools & Skills tab, live-console tool calls, `make e2e-smoke-m10`, docs.

Live verification of `claude-code`, `codex`, and `kilo-code` wiring remains a manual acceptance item (needs their CLIs installed), not a plan task.

1. **Connector verification:** stub `/mcp` with header token; prove all five real CLIs can connect and call a tool; record baseline context sizes. Update the wiring table.
2. Gateway, tokens, core read tools, `result_submit`, `run_tool_calls`; mock `toolCalls`.
3. Built-in `coppice` plugin skills, `SkillCatalog`, slim context builders, migrate role rules.
4. Wire all connectors; switch to tool-first; delete legacy fat-context builders and `.agent/*.json` once all six pass.
5. Plugin dirs, scan, git install, enablement, agent assignment (API + UI + presets).
6. Plugin MCP proxy, plugin settings, Test button.
7. Tools & Skills tab, live console rendering, smoke, docs (`AGENTS.md`, `docs/architecture.md`, `docs/providers/`).

## Risks

| Risk | Mitigation |
|------|------------|
| Agent forgets `result_submit` | Final-JSON fallback; prompt + context one-liner |
| Agent skips a required role skill | Must-rules remain one-liners in context; required skill named explicitly; Skills Used visible |
| Connector MCP quirks / version drift | Verification is task 1; `mcp_unavailable` fails loudly |
| Shared plugin server state across runs | Documented; per-run instances possible later |
| Arbitrary plugin code before M11 | Disabled by default, admin-only enable, explicit warning, M11 sandboxes processes |
| Server-side git clone for plugins | Scoped exception; repositories rule unchanged |

## Acceptance criteria

- [ ] Connector verification recorded; wiring table updated
- [ ] Plugin dirs, scan, git install, enable, agent assignment work via UI and API
- [ ] Claude Code / Cursor format plugins and skills-only folders load unchanged; unsupported parts listed
- [ ] `/mcp` gateway with per-run tokens; core tools and `result_submit` per profile matrix
- [ ] Plugin MCP servers proxied with namespacing, health, restart, settings
- [ ] All six connectors run tool-first; legacy fat context removed
- [ ] `full` context ≥50% smaller than baseline on fixture tickets
- [ ] Knowledge Used, Skills Used, and tool calls visible in Agent Run detail
- [ ] M05 / M06 / M09 behavior unchanged; existing smokes pass
- [ ] `make test`, clippy, `make web-test`, `make e2e-smoke-m10` pass
