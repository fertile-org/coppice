# Claude Code provider

**ID:** `claude-code`  
**Status:** Implemented  
**Stream backend:** subprocess stdout (stream-json)

Anthropic Claude Code CLI integration via subprocess. Agents run as `claude -p` processes that inherit the server's environment.

## Auth

Auth is **host-managed**, exactly like the opencode connector. The operator runs `claude login` (subscription) or sets `ANTHROPIC_API_KEY` wherever the server runs, and the spawned `claude` child process inherits that environment directly. Coppice does not inject or strip credentials.

```toml
[agent.connectors.claude-code]
enabled = true
# run_timeout_secs = 600
# model_providers = ["sonnet", "opus", "haiku"]
```

## Docker

The server image does not include `claude`. Mount the host CLI + auth yourself once (overview: [Docker Compose (host CLIs)](README.md#docker-compose-host-clis)).

**1. Host prep**

```bash
claude login   # or export ANTHROPIC_API_KEY on the server instead of mounting auth
claude --version
```

**2. Enable in** `deploy/config/config.toml`

```toml
[agent.connectors.claude-code]
enabled = true
model_providers = ["sonnet", "opus", "haiku"]
```

**3. Compose override** — save as `deploy/docker-compose.claude.yml` (local only):

```yaml
services:
  server:
    environment:
      HOME: ${HOME}
      PATH: ${HOME}/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
      # Optional instead of mounting ~/.claude:
      # ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}
    volumes:
      - ${HOME}/.local/bin:${HOME}/.local/bin:ro
      # Claude install tree when `claude` is a symlink into ~/.local/share/claude
      - ${HOME}/.local/share/claude:${HOME}/.local/share/claude:ro
      - ${HOME}/.claude:${HOME}/.claude:ro
```

**4. Apply**

```bash
docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.claude.yml \
  up -d --force-recreate server

docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.claude.yml \
  exec -u "$(id -u):$(id -g)" server claude --version
```

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `claude -p --output-format stream-json --verbose` |
| Session ID capture | Extracted from stream-json events, persisted to run |
| Model selection | `--model` from agent config |
| Result parsing | `extract_result_from_text` on final assistant / result text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as `Frame` messages |
| Session resume | `--resume <session_id>` on continuation runs |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `claude -p "<coppice_run_prompt>" --output-format stream-json --verbose --allowedTools ... --permission-mode bypassPermissions` with CWD set to the agent worktree.
2. Each stdout line is a JSON event. The provider accumulates assistant text deltas and captures `session_id` from the first event that contains it.
3. The terminal `result` event provides the final assistant text. Coppice extracts the JSON contract (`AgentRunResult`) from that text using `extract_result_from_text`.
4. On cancel or timeout, the subprocess is killed.

## Live streaming (WebSocket console)

Claude Code's `--output-format stream-json` emits newline-delimited JSON events on stdout. Each event is mapped to a `LiveMessage` variant and forwarded to the `RunStreamHandle` in real time:

| Stream-JSON event | LiveMessage variant | Notes |
|-------------------|---------------------|-------|
| `system` (subtype `init`) | `Frame` | Session start line; `session_id` captured via `session_created_tx` |
| `system` (subtype `api_retry`) | `Frame` | Retry notice |
| `assistant` / `user` (`text` content) | `Frame` | Assistant prose or final JSON contract |
| `assistant` / `user` (`tool_use`) | `Frame` | e.g. `▸ Read: path`, `$ cargo test` |
| `assistant` / `user` (`tool_result`) | `Frame` | Truncated tool output (`✓` / `✗`) |
| `result` (terminal event) | `Frame` | Formatted result card (Done/Blocked + summary + meta lists); duplicate skipped |

Frames are published via `RunStreamHandle::publish_frame(seq, data)` where `seq` is a monotonic counter. The `RunStreamHandle` broadcasts to all WebSocket subscribers and retains a 500-message ring buffer for late-replay.

**Recovery after server restart:** The subprocess is gone, so live reattach is not possible. The WS live endpoint replays the persisted `terminal.log` artifact as a single `Frame` message. If the run was still active, it is marked interrupted.

## Session resume

When a `Continued` result leads to a follow-up run, the job worker looks up the previous run's `session_id` from `agent_runs` and passes it to the connector as `resume_session_id`. The connector adds `--resume <session_id>` to the claude command, which restores the full conversation context within claude-code's session store.

## Context compaction

Claude Code compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).
