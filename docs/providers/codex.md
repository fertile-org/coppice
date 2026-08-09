# Codex provider

**ID:** `codex`
**Status:** Implemented
**Stream backend:** subprocess stdout (JSONL)

OpenAI Codex CLI integration via subprocess. Agents run as `codex exec` processes that inherit the server's environment.

## Auth

Auth is **host-managed**, exactly like the opencode and claude-code connectors. The operator runs `codex login` (subscription) or sets the appropriate environment variable wherever the server runs, and the spawned `codex` child process inherits that environment directly. Coppice does not inject or strip credentials.

```toml
[agent.connectors.codex]
enabled = true
# run_timeout_secs = 600
# model_providers = ["openai", "azure"]
```

## Setup

```bash
coppice connector enable codex
coppice connector install codex
coppice connector setup codex     # codex login --device-auth
coppice connector doctor codex
```

See [M08](../milestones/M08-connector-operator-cli.md).

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `codex exec --json --dangerously-bypass-approvals-and-sandbox` |
| Thread ID capture | Extracted from `thread.started` event, persisted to run |
| Model selection | `-m` / `--model` from agent config |
| Result parsing | `extract_result_from_text` on accumulated agent_message text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as `Frame` messages |
| Session resume | `codex exec resume <thread_id>` on continuation runs (unreliable) |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `codex exec --json --dangerously-bypass-approvals-and-sandbox -C <worktree> -m <model>` and writes the prompt to stdin.
2. Each stdout line is a JSON event. The provider accumulates agent_message text from `item.completed` events and captures `thread_id` from the `thread.started` event.
3. The terminal `turn.completed` event signals completion. Coppice extracts the JSON contract (`AgentRunResult`) from the accumulated text using `extract_result_from_text`.
4. On cancel or timeout, the subprocess is killed.

## Live streaming (WebSocket console)

Codex's `--json` emits newline-delimited JSON events on stdout. Each event is mapped to a `LiveMessage` variant and forwarded to the `RunStreamHandle` in real time:

| Codex event | LiveMessage variant | Notes |
|-------------|---------------------|-------|
| `thread.started` | — | Contains the `thread_id`; captured early via `session_created_tx` |
| `item.completed` (type: `agent_message`) | `Frame` | Display text extracted from `item.text` |
| `turn.completed` | — | Terminal event; signals loop break |
| Other events | — (ignored for display) | Not forwarded as frames |

Frames are published via `RunStreamHandle::publish_frame(seq, data)` where `seq` is a monotonic counter. The `RunStreamHandle` broadcasts to all WebSocket subscribers and retains a 500-message ring buffer for late-replay.

**Recovery after server restart:** The subprocess is gone, so live reattach is not possible. The WS live endpoint replays the persisted `terminal.log` artifact as a single `Frame` message. If the run was still active, it is marked interrupted.

## Session resume

Session resume for codex is **unreliable**—the Codex CLI implementation for session continuation is not stable. The connector includes `codex exec resume <thread_id>` support (following the same pattern as claude-code), but cross-run continuity should rely on checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

**Note:** Session resume for codex is NOT supported in the job worker's `load_resume_session_id` function (unlike claude-code). The `load_resume_session_id` function only returns session IDs for claude-code runs; codex relies on the checkpoint-based continuation pattern.

## Context compaction

Codex compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## Models

Model options are loaded dynamically from the installed Codex CLI:

```bash
codex debug models
```

Coppice calls this at `GET /api/connectors/codex/model-providers/{provider}/models` and returns models whose `visibility` is not `hide`, filtered by provider:

| Provider | Filter |
|----------|--------|
| `openai` | Slugs without an `azure/` prefix (e.g. `gpt-5.5`, `gpt-5.4`) |
| `azure` | Slugs with an `azure/` prefix |

The list reflects your Codex CLI version and login — it is not hardcoded in Coppice. If the CLI is missing or not logged in, the models endpoint returns an error.
