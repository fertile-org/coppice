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
| `missing_config` | Model provider missing from connector `model_providers` |

Unreachable or misconfigured agents are not used for new auto-assignments until fixed.

## Coppice MCP gateway (tool-first runs)

Every run gets a per-run bearer token for the Coppice MCP gateway at `POST /mcp`, handed to the CLI as `COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN`. Connectors configure the gateway per run only — CLI flags, per-process env, or a config file in that run's artifacts dir (`<artifacts_dir>/runs/<run id>/`). Nothing is written to the worktree, a registered repo checkout, or the operator's global CLI config, and the token is never written to a file in plaintext (each config interpolates it from the environment). A connector that cannot be configured fails the run with `mcp_unavailable` — there is no fallback to the old fat context.

| Connector | Status | Mechanism |
|-----------|--------|-----------|
| `mock` | n/a | Fixture `toolCalls` executed over HTTP JSON-RPC against `/mcp` |
| `cursor` | **verified** (CLI `2026.09.28-64d2043`) | Per-run `HOME` with `.cursor/mcp.json`, plus `CURSOR_CONFIG_DIR` with a `cli-config.json` that allows `Mcp(coppice:*)` |
| `claude-code` | unverified — expected mechanism | `--mcp-config <run dir>/mcp.json --strict-mcp-config`; `mcp__coppice__*` added to `--allowedTools` |
| `codex` | unverified — expected mechanism | `-c mcp_servers.coppice.url=…` + `-c mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"` |
| `kilo-code` | unverified — expected mechanism | `KILO_CONFIG` pointing at `<run dir>/kilo-config.json` (OpenCode-style `mcp` block, `{env:COPPICE_MCP_TOKEN}` header) |
| `opencode` | **not tool-first** | Refuses with `mcp_unavailable` |

Verify an unverified row when its CLI is first available: run one ticket and confirm `run_tool_calls` records `ticket_get` and `result_submit`. For `kilo-code` the env var name follows the OpenCode `OPENCODE_CONFIG` convention and may differ in the fork.

**Cursor side effects.** Overriding `HOME` also changes it for the agent's own shell commands, so the connector forwards `XDG_CONFIG_HOME` (the operator's real config home, where `cursor/auth.json` lives) and, when they exist and are not already set, `GIT_CONFIG_GLOBAL` and `GH_CONFIG_DIR`.

**OpenCode follow-up.** The shared `opencode serve` process is started once for the server, so it cannot carry a per-run MCP server or token. Making OpenCode tool-first means switching the connector to per-run `opencode run` processes with a per-run `OPENCODE_CONFIG`, like `kilo-code`.

Design: [M10 plugins](../superpowers/specs/2026-09-29-m10-plugins-design.md).

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

See [architecture.md](../architecture.md) (server `providers/`, thin API handlers) and the existing docs above as templates. Prefer a dedicated provider module and live model listing when the CLI supports it.
