# Cursor provider

**ID:** `cursor`  
**Status:** Implemented  
**Stream backend:** subprocess stdout (stream-json)

Cursor Agent CLI integration via subprocess. Agents run as `agent -p` processes that inherit the server's environment.

## Auth

Auth is **host-managed**, exactly like the claude-code and codex connectors. The operator runs `agent login` wherever the server runs, and the spawned `agent` child process inherits that environment directly. Coppice does not inject or strip credentials.

```toml
[agent.connectors.cursor]
enabled = true
command = "agent"
# run_timeout_secs = 600
model_providers = ["cursor"]
```

## Docker

The server image does not include `agent`. For Compose, mount the host CLI + auth yourself once — same manual step as other connectors. See [Docker Compose (host CLIs)](README.md#docker-compose-host-clis).

Typical pieces: `~/.local/bin/agent`, `~/.local/share/cursor-agent`, `~/.config/cursor` after `agent login` on the host. Set container `HOME` so the CLI finds auth.

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `agent -p --trust --force --output-format stream-json --workspace <worktree>` |
| Session ID capture | Extracted from stream-json events, persisted to run |
| Model selection | `--model` from agent config |
| Result parsing | `extract_result_from_text` on terminal `result` event text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as `cursor.console.*` events |
| Session resume | `--resume <session_id>` on continuation runs |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `agent -p "<coppice_run_prompt>" --trust --force --output-format stream-json --workspace <worktree>` with CWD set to the agent worktree.
2. `--model <model>` is added when configured on the agent. `--resume <session_id>` is added when the job worker supplies a prior run's session id.
3. Each stdout line is a JSON event. The provider captures `session_id` from the first event that contains it and forwards events to `CursorConsolePublisher` for live display.
4. The terminal `type: "result"` event provides the final text. Coppice extracts the JSON contract (`AgentRunResult`) from that text using `extract_result_from_text`. If `is_error` is true or the subtype indicates failure, the run fails with a clear error.
5. On cancel or timeout, the subprocess is killed.

## Live streaming (WebSocket console)

Cursor's `--output-format stream-json` emits newline-delimited JSON events on stdout. Each event is mapped to a `cursor.console.*` event and forwarded to the `RunStreamHandle` in real time:

| Stream-JSON event | Console event | Notes |
|-------------------|---------------|-------|
| `system` (subtype `init`) | `cursor.console.session` | Session start; `session_id` captured via `session_created_tx` |
| `assistant` (`message.content[].text`) | `cursor.console.text` | Assistant prose; markdown payload |
| `tool_call` (subtype `started`) | `cursor.console.tool` | Running tool summary (shell command or file path) |
| `tool_call` (subtype `completed`) | `cursor.console.tool` | Completed or error status; optional output |
| `result` | `cursor.console.text` / `cursor.console.result` | Result text; contract emitted once when JSON contract is detected |
| `thinking`, `user` | — (ignored) | Not forwarded as console events |

Events are published via `RunStreamHandle::publish` as `LiveMessage::Event`. The `RunStreamHandle` broadcasts to all WebSocket subscribers and retains a 500-message ring buffer for late-replay.

**Recovery after server restart:** The subprocess is gone, so live reattach is not possible. The WS live endpoint replays the persisted `terminal.log` artifact as a single `Frame` message. If the run was still active, it is marked interrupted.

## Session resume

When a `Continued` result leads to a follow-up run, the job worker looks up the previous run's `session_id` from `agent_runs` and passes it to the connector as `resume_session_id`. The connector adds `--resume <session_id>` to the agent command, which restores the full conversation context within Cursor's session store. This wiring matches `claude-code`.

## Models

Model options are loaded dynamically from the installed Cursor Agent CLI:

```bash
agent models
```

Coppice calls this at `GET /api/connectors/cursor/model-providers/cursor/models` and returns model ids for the Agents UI. The connector uses a single synthetic model provider id: `cursor`. Operators set `model_providers = ["cursor"]` in config.

The list reflects your Cursor CLI version and login — it is not hardcoded in Coppice. If the CLI is missing or not logged in, the models endpoint returns an error.

## Limitations

- **Docker / PATH:** The `agent` binary and login state must be available to the server process. Host install: put `agent` on PATH and run `agent login`. Compose: mount CLI + auth manually ([Docker Compose (host CLIs)](README.md#docker-compose-host-clis)).
- **No Cursor worktree flag:** Coppice already owns worktrees. The connector uses `--workspace <coppice-worktree>` and process CWD; it never passes Cursor's `-w` / `--worktree`.
- **No SDK:** This connector drives the CLI subprocess only. It does not use `@cursor/sdk`, `cursor-sdk`, or cloud/private worker integrations.
- **MCP injection:** Not supported in v1 (follow-up ticket).
- **Live recovery:** Not possible after a server restart (subprocess is gone). The persisted `terminal.log` is replayed instead.
