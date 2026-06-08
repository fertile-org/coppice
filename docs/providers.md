# Agent providers

Coppice runs agents through a **provider adapter** layer. Orchestration (queue, worktrees, result contract, ticket updates) is provider-agnostic; each adapter knows how to execute, stream output, and parse results.

## M04 status

| Provider | Status | Stream backend | Notes |
|----------|--------|----------------|-------|
| `mock` | **Implemented** (M03) | `ScriptedStream` | Default for CI and `deploy/docker-compose.yml` |
| `opencode` | **Implemented** (M04) | `OpenCodeStream` | Auto-starts `opencode serve`; manual testing with API keys |
| `claude-code` | **Deferred** | `TmuxStream` (future) | User pre-authenticates via `claude auth` |
| `codex` | **Deferred** | `TmuxStream` (future) | Subscription/OAuth; session resume unreliable |
| `shell` | **Deferred** | `TmuxStream` or direct | Custom command wrapper |

## Configuration

```toml
[agent]
default_provider = "mock"

[agent.providers.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
```

Set `default_provider = "opencode"` only on your host `config.toml` for manual testing. Never in CI or the agent Docker stack.

## OpenCode (M04)

**Auth:** run `opencode auth login` on the host. Coppice does not store API keys.

**Execution:**

1. Server auto-starts `opencode serve` on startup (when enabled).
2. Per run: `opencode run --attach http://127.0.0.1:{port} --dir <worktree>`.
3. Agent reads `.agent/context.md` (includes result contract JSON schema).

**Streaming:** OpenCode JSON/SSE events → normalized terminal frames → Live Console.

**Future TODO — on-demand serve lifecycle:**

- Stop `opencode serve` after an idle period with no queued/running jobs.
- Restart serve when the next OpenCode job is claimed.
- Reduces idle resource use without manual process management.

**Future TODO — manual attach:**

- `attach_url` config to connect to an externally managed `opencode serve` instead of auto-start.

## Claude Code (deferred)

Modeled after [Multica's provider matrix](https://www.multica.ai/docs/providers).

| Capability | Status |
|------------|--------|
| Auth | User runs `claude auth` outside Coppice |
| Live stream | Future: `TmuxStream` (raw pane capture) |
| Session resume | Expected to work |
| MCP injection | Future: `--mcp-config` per agent |
| Result parsing | Best-effort JSON extraction from terminal tail |

Coppice cannot inject Anthropic subscription credentials. The operator must authenticate the CLI before running agents.

## Codex (deferred)

| Capability | Status |
|------------|--------|
| Auth | User subscription / OAuth via Codex CLI |
| Live stream | Future: `TmuxStream` |
| Session resume | Unreliable (Multica: "code exists but unreachable") |
| MCP injection | Future: per-task `$CODEX_HOME/config.toml` |
| Result parsing | Best-effort JSON from terminal tail |

Harder to control than API-key providers. Document first, implement when `TmuxStream` and stop/kill are stable.

## Comparison with Multica

Multica uses a **local daemon** that polls a remote server and spawns CLIs on the operator's machine. Coppice is **self-hosted monolith**: the in-process worker runs on the same host as the CLIs (human hot-reload dev) or uses mock in Docker CI.

Shared lessons from Multica:

- Per-provider adapter files with an honest capability matrix.
- API-capable tools (OpenCode) use structured event streams, not tmux capture.
- Subscription CLIs (Claude, Codex) need tmux or hook-based observation.
- Do not assume bare `exec` stdout works for all CLI versions; prefer attach/API modes.

## Testing rules

- **CI / E2E:** always `default_provider = "mock"`.
- **Integration tests:** mock only; no network LLM calls.
- **Manual:** OpenCode with your own API keys on host `config.toml`.
