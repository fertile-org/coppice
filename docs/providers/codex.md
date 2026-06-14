# Codex provider

**ID:** `codex`
**Status:** Implemented
**Stream backend:** subprocess stdout (stream-json)

OpenAI Codex CLI integration via subprocess. Agents run as `codex -p` processes that inherit the server's environment.

## Auth

Auth is **host-managed**, exactly like the opencode and claude-code connectors. The operator runs `codex login` (subscription) or sets the appropriate environment variable wherever the server runs, and the spawned `codex` child process inherits that environment directly. Coppice does not inject or strip credentials.

```toml
[agent.connectors.codex]
enabled = true
# run_timeout_secs = 600
# model_providers = ["openai", "azure"]
```

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `codex -p --output-format stream-json --verbose` |
| Session ID capture | Extracted from stream-json events, persisted to run |
| Model selection | `--model` from agent config |
| Result parsing | `extract_result_from_text` on final assistant / result text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as `Frame` messages |
| Session resume | `--resume <session_id>` on continuation runs (unreliable) |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `codex -p "<coppice_run_prompt>" --output-format stream-json --verbose --allowedTools ... --permission-mode bypassPermissions` with CWD set to the agent worktree.
2. Each stdout line is a JSON event. The provider accumulates assistant text deltas and captures `session_id` from the first event that contains it.
3. The terminal `result` event provides the final assistant text. Coppice extracts the JSON contract (`AgentRunResult`) from that text using `extract_result_from_text`.
4. On cancel or timeout, the subprocess is killed.

## Live streaming (WebSocket console)

Codex's `--output-format stream-json` emits newline-delimited JSON events on stdout. Each event is mapped to a `LiveMessage` variant and forwarded to the `RunStreamHandle` in real time:

| Stream-JSON event | LiveMessage variant | Notes |
|-------------------|---------------------|-------|
| `system` (subtype `init`) | `Frame` | Contains the `session_id`; captured early via `session_created_tx` |
| `assistant` (message with text content) | `Frame` | Display text extracted from `message.content[].text` parts |
| `result` (terminal event) | `Frame` | Final result text; signals loop break |
| Tool / other events | — (ignored for display) | Not forwarded as frames |

Frames are published via `RunStreamHandle::publish_frame(seq, data)` where `seq` is a monotonic counter. The `RunStreamHandle` broadcasts to all WebSocket subscribers and retains a 500-message ring buffer for late-replay.

**Recovery after server restart:** The subprocess is gone, so live reattach is not possible. The WS live endpoint replays the persisted `terminal.log` artifact as a single `Frame` message. If the run was still active, it is marked interrupted.

## Session resume

Session resume for codex is **unreliable**—the Codex CLI implementation for session continuation is not stable. The connector includes `--resume <session_id>` support (following the same pattern as claude-code), but cross-run continuity should rely on checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## Context compaction

Codex compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## Models

Known models by provider:

| Provider | Models |
|----------|--------|
| `openai` | `gpt-4o`, `gpt-4o-mini`, `o1`, `o1-mini` |
| `azure` | `azure/gpt-4o`, `azure/gpt-4o-mini`, `azure/o1` |

Model availability depends on the Codex CLI version and the configured API credentials.
