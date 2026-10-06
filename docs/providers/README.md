# Agent connectors

Coppice runs agents through **connectors**. Each connector talks to a different CLI or service (Cursor, Claude Code, OpenCode, …). Tickets, the board, and worktrees stay the same; you pick a connector per agent.

| Connector | Doc | Notes |
|-----------|-----|--------|
| `mock` | [mock.md](mock.md) | Default for CI and Compose smoke — no real CLI |
| `cursor` | [cursor.md](cursor.md) | Cursor Agent CLI (`agent`) |
| `claude-code` | [claude-code.md](claude-code.md) | Claude Code CLI (`claude`) |
| `codex` | [codex.md](codex.md) | OpenAI Codex CLI (`codex`) |
| `opencode` | [opencode.md](opencode.md) | OpenCode serve + Live Session UI |
| `kilo-code` | [kilo-code.md](kilo-code.md) | Kilo CLI (`kilo`) |
| `shell` | [shell.md](shell.md) | Deferred |

**Source of truth.** Connector facts (id, binary, auth hints, MCP wiring style, live console kind, capabilities such as chat resume and read-only enforcement) live in one descriptor table, [`connectors/src/lib.rs`](../../connectors/src/lib.rs), read by the server, the `coppice connector` CLI, and the web (through `GET /api/connectors`). The tables on this page describe it; when they disagree, the descriptor wins. Layer overview: [architecture.md § Connector layer](../architecture.md#connector-layer).

## Connectors vs model providers vs models

| Layer | Example | Where you set it |
|-------|---------|------------------|
| Connector | `cursor`, `opencode` | Agent in the UI, and `[agent.connectors.*]` in config |
| Model provider | `cursor`, `zai-coding-plan` | `model_providers = [...]` in connector config |
| Model | a specific model id | Per agent in the UI (fetched live after login) |

## Docker Compose

To use a real connector, follow that connector’s doc **One-time setup** section (run `coppice connector …` on the **server** container). Example for Cursor: [cursor.md § One-time setup](cursor.md#one-time-setup-docker-compose).

Pattern for every connector:

1. `enable` → recreate the server  
2. `install` → `setup` → `doctor`  
3. Create an agent in the UI and pick connector / provider / model  
4. Verify from **Tools → Connectors → Test connection** (see [Diagnostics](#diagnostics-tools--connectors))  

Binaries and login state live in a Compose volume at `/home/coppice`. You do not mount host `~/.local` / `~/.config` for CLIs. Default Compose stays on `mock` for CI.

Design notes: [M08](../milestones/M08-connector-operator-cli.md).

## Per-agent choice

`[agent] default_connector` sets the default. Each agent can override connector, model provider, and model on the Agents page. At run time the worker uses that agent’s values.

**Health** (separate from enabled/disabled):

| Health | Meaning |
|--------|---------|
| `unknown` | Check not run yet |
| `healthy` | Connector reachable and model provider configured |
| `unreachable` | Connector/CLI not usable |
| `missing_config` | Connector turned off, or model provider missing from connector `model_providers` |

Unreachable or misconfigured agents are not used for new auto-assignments until fixed.

## Coppice MCP gateway (tool-first runs)

Every run gets a per-run bearer token for the Coppice MCP gateway at `POST /mcp`, handed to the CLI as `COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN`. Connectors configure the gateway per run only — CLI flags, per-process env, or a config file in that run's artifacts dir (`<artifacts_dir>/runs/<run id>/`). Nothing is written to the worktree, a registered repo checkout, or the operator's global CLI config, and the token is never written to a file in plaintext (each config interpolates it from the environment). A connector that cannot be configured fails the run with `mcp_unavailable` — there is no fallback to the old fat context.

| Connector | Status | Mechanism |
|-----------|--------|-----------|
| `mock` | n/a | Fixture `toolCalls` executed over HTTP JSON-RPC against `/mcp` |
| `cursor` | **verified** (CLI `2026.09.28-64d2043`) | Coppice-owned `HOME` with `.cursor/mcp.json`, plus `CURSOR_CONFIG_DIR` with a `cli-config.json` that allows `Mcp(coppice:*)` (see the state-directory note below) |
| `claude-code` | unverified — expected mechanism | `--mcp-config <run dir>/mcp.json --strict-mcp-config`; `mcp__coppice__*` added to `--allowedTools` |
| `codex` | unverified — expected mechanism | `-c mcp_servers.coppice.url=…` + `-c mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"` |
| `kilo-code` | unverified — expected mechanism | `KILO_CONFIG` pointing at `<run dir>/kilo-config.json` (OpenCode-style `mcp` block, `{env:COPPICE_MCP_TOKEN}` header) |
| `opencode` | **verified** (`1.18.33`) | Per-run `opencode serve` with `OPENCODE_CONFIG=<run dir>/opencode.json` (remote `coppice` MCP server, `{env:COPPICE_MCP_TOKEN}` header) |

Verify an unverified row when its CLI is first available: create an agent on that connector, then use **Tools → Connectors → Test connection** (below). A pass means the run called `ticket_get` and `result_submit` through the gateway; record the CLI version in the table. For `kilo-code` the env var name follows the OpenCode `OPENCODE_CONFIG` convention and may differ in the fork — a check failing with `mcp_unavailable` points there.

## Diagnostics (Tools → Connectors)

Admins open **Tools → Connectors** (`/tools?tab=connectors`) to see, per connector except `mock`:

| Row | Meaning |
|-----|---------|
| Enabled | A switch. Turning it on or off is written into `config.toml` (comments and other keys kept) and the running server picks it up immediately. Saving an agent on a turned-off connector turns that connector on the same way. `coppice connector enable` and hand-edits apply the next time the server starts. |
| CLI | **Ready** (binary found and sign-in verified), **Found, not signed in**, or **Not on your PATH**. A binary with no reliable sign-in check shows the path and no status claim. |
| Auth | **Detected** (which auth env var **names** are set and which auth files under HOME exist — never values or file contents), **Verified by probe**, or **Not found**. This row is separate from the CLI status above. |
| Probe | Shown when the CLI is found: the first line of the probe output (e.g. the version), **Failed** with up to 500 chars of output, or **Timed out** (10 s). |
| Last real run | The latest finished non-check run of an agent on this connector, with whether it made an `ok` `ticket_get` and `result_submit` call. |
| Last test | The latest Test connection: time, passed/failed, and the failure reason. |

When the CLI or auth is missing, the card shows the connector's auth hint and a vendor install docs link. Coppice never installs a CLI or runs a login from the page — use the `coppice connector …` steps above.

Probes run at server startup, on **Run check**, and again when **Test connection** starts (the test itself is not delayed for the probe). The page shows the cached result and does not probe on every load. Until the startup probe finishes a card shows "Checking…".

Sign-in is a separate cheap check (a few seconds, no browser, no prompt, no model call): credential files or the CLI's own non-interactive status command. **Kilo Code has no such check** — `kilo auth` is a TUI — so a found `kilo` binary is never labeled "Found, not signed in" or "Ready" from auth. Cursor, Claude Code, Codex, and OpenCode do have a check.

Turning a connector off does not stop a run that is already in progress. A queued run, a new ticket run, chat, a connector test, and knowledge compaction fail immediately with a message that names the connector and points at Tools → Connectors. Agent health shows the same message.

**Test connection** runs a real agent run through the production path (provider adapter, per-run MCP wiring, token, gateway, `run_tool_calls`). Pick an agent that uses the connector (create one on the Agents page first). The run gets a scratch directory and a two-tool profile — `ticket_get` returns a fixed synthetic ticket and `result_submit` — so it never reads or changes a real ticket, repository, comment, or notification. It times out after the connector's `run_timeout_secs` or 180 s, whichever is shorter. The check passes when the run calls both tools and submits `done`; otherwise it fails with the first reason that applies:

| Failure | Likely cause |
|---------|--------------|
| The run error (e.g. `mcp_unavailable`, `connection check timed out after …`) | CLI could not be configured or started, not logged in, or the model never finished |
| `ticket_get was not called` | The CLI did not see the Coppice MCP server (wiring) or ignored it |
| `result_submit was not called` | The agent stopped before submitting |
| `result was <outcome>` | The agent's first valid submission was not `done` (only the first one counts) |
| `server restarted` | The server restarted while the check was queued or running |

Only one check per connector runs at a time. The CLI still has `coppice connector doctor <id>` with the same local checks for terminal use.

**Cursor state directory.** The CLI keeps its `chats` state under `CURSOR_CONFIG_DIR`, so the per-run home and config dirs are keyed by whatever `--resume` resolves against, not by run id:

| Run | Directory under the artifacts dir |
|-----|-----------------------------------|
| Chat turn | `chat-sessions/<chat session id>/cursor-{home,config}` |
| Ticket run | `tickets/<ticket id>/cursor-{home,config}` |
| Anything else (no resume) | `runs/<run id>/cursor-{home,config}` |

`mcp.json` and `cli-config.json` are rewritten every turn; neither holds the token, so runs sharing a directory cannot corrupt each other. All paths are absolutized first — `storage.artifacts_dir` is usually relative and the CLI is spawned with the worktree as its working directory.

**Cursor side effects.** Overriding `HOME` also changes it for the agent's own shell commands, so the connector forwards `XDG_CONFIG_HOME` (the operator's real config home, where `cursor/auth.json` lives) and, when they exist and are not already set, `GIT_CONFIG_GLOBAL` and `GH_CONFIG_DIR`. Because `CURSOR_CONFIG_DIR` points at a Coppice-owned directory, permission rules in the operator's own `cli-config.json` do **not** apply to Coppice runs — only the `Mcp(coppice:*)` allow rule Coppice writes does.

**OpenCode per-run server.** Each run (and each chat turn) spawns its own `opencode serve` on a free port on `serve_hostname`, with `OPENCODE_CONFIG` pointing at `<artifacts_dir>/runs/<run id>/opencode.json` (runs without a gateway token, such as drafts, use a temp file). The process is killed when the run ends, is cancelled, or fails, and on server shutdown. `serve_port` is ignored. Sessions live in OpenCode's shared data dir, so a later run's process resumes an existing session id. A per-run server does not survive a Coppice restart, so active OpenCode runs are marked interrupted on startup.

**Plugin tools.** Plugin MCP servers are proxied through the same `coppice` server as `<plugin>__<tool>`, so connectors need no per-plugin wiring. See [architecture.md § Plugin MCP proxy](../architecture.md#plugin-mcp-proxy).

**Console tool titles.** Each live console renders gateway calls as `coppice · <tool>` for core tools and `<plugin> · <tool>` for plugin tools. The connector's descriptor `mcp_tool_names` (`ToolNameStyle`) says how its CLI spells gateway tool names; `coppice_connectors::gateway_tool` (or `gateway_tool_from_fields` for connectors that report the server as a separate field, like codex) strips that prefix and splits plugin from tool on the first `__`. A new connector's console needs only the right style — no console code.

| `ToolNameStyle` | CLI spelling | Connectors |
|-----------------|--------------|------------|
| `McpDoubleUnderscore` | `mcp__coppice__<tool>` | `claude-code` |
| `Dash` | `coppice-<tool>` | `cursor` |
| `Underscore` | `coppice_<tool>` | `opencode`, `kilo-code` (Kilo emits no tool events) |
| `ServerToolFields` | separate `server` / `tool` fields | `codex` |
| `None` | — | `mock` |

Design: [M10 plugins](../superpowers/specs/2026-09-29-m10-plugins-design.md), [Part 2b plugin MCP](../superpowers/specs/2026-10-02-m10-part2b-plugin-mcp-design.md).

## Agent Chat multi-turn (provider session resume)

Agent Chat reuses the vendor session across human messages when possible. Coppice still stores the full transcript in Postgres; on later turns the worker passes `--resume` / OpenCode session reuse with a **slim** `.agent/context.md`. If resume fails, one automatic retry sends the full transcript (logged as `chat_resume_fallback`).

| Connector | Resume | Read-only enforcement in chat |
|-----------|--------|-------------------------------|
| `mock` | Yes (CI) | Fixtures |
| `claude-code` | `--resume` | `--allowedTools` read-only allowlist |
| `cursor` | `--resume` | `--mode ask` |
| `codex` | `codex exec resume` (best-effort) | Prompt + rules only; CLI uses bypass flag — see [codex.md](codex.md) |
| `opencode` | HTTP session `prompt_async` | Prompt + rules only — see [opencode.md](opencode.md) |
| `kilo-code` | — | Not supported for chat |

Design: [Agent Chat provider session resume](../superpowers/specs/2026-09-28-agent-chat-provider-session-resume-design.md).

## Adding a connector

Follow the checklist in [architecture.md § Adding a connector](../architecture.md#adding-a-connector): descriptor entry (including probe args, `probe_proves_auth`, and `docs_url`, which drive `doctor` and the Connectors page), config struct and example config sections, adapter (`run_cli` + `LineHandler`, or a custom `AgentProvider`) with a `ModelCatalog`, `FACTORIES` entry, a wiring renderer only for a new MCP style, plus the remaining manual touchpoints it lists (the config `enabled(id)` arm, read-only list, per-id CLI arms in `enable.rs` / `install.rs` / `setup.rs`). Use the existing docs above as templates. Prefer live model listing when the CLI supports it.
