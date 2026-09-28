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
