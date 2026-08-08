# Kilo Code provider

**ID:** `kilo-code`
**Status:** Implemented (subprocess; manual host verification required)
**Stream backend:** subprocess stdout (JSON events)

Kilo Code CLI integration via subprocess. Agents run as `kilo run --format json --auto` processes that inherit the server's environment.

## Discovery summary (target version: Kilo CLI 1.0+, `@kilocode/cli`)

The Kilo CLI is installed via `npm install -g @kilocode/cli` and launched as `kilo`. The public docs describe:

- `kilo run [message..]` — non-interactive run with the message passed as a **positional arg** (no documented stdin mode).
- `--format json` — machine-readable "raw JSON events" on stdout.
- `-m` / `--model` — model in `provider/model` format.
- `--continue` / `-c` and `--session <id>` / `-s <id>` — resume the last or a specific session.
- `--auto` — auto-approve permissions for autonomous / pipeline usage. `--dangerously-skip-permissions` also exists.
- `kilo models [provider]` — list available models (live, from the installed CLI and credentials).
- Exit codes: `0` success, `124` timeout, `1` error.

Kilo is a documented OpenCode fork and the config docs state it supports the same configuration options as OpenCode. However, the public docs do **not** document a stable HTTP/SSE daemon API surface for `kilo serve` / `kilo daemon`, and OpenCode endpoint compatibility (session/message/event semantics) is **not confirmed**. Per the ticket's decision rule, this connector therefore uses the subprocess path rather than the OpenCode daemon client. The `--format json` event schema is OpenCode-derived but not version-pinned in CI, so event parsing is defensive (see [How it works](#how-it-works)).

References:
- https://kilo.ai/docs/code-with-ai/platforms/cli
- https://kilo.ai/docs/code-with-ai/platforms/cli-reference
- https://github.com/Kilo-Org/kilocode

## Auth

Auth is **host-managed**, exactly like the opencode, claude-code, and codex connectors. The operator runs `kilo` → `/connect` (or `kilo auth login <url>`) wherever the server runs, and the spawned `kilo` child process inherits that environment directly. Coppice does not inject or strip credentials and does not store Kilo API keys or account secrets in its config.

```toml
[agent.connectors.kilo-code]
enabled = true
command = "kilo"
# run_timeout_secs = 600
model_providers = ["anthropic", "openai"]
```

## Docker

The server image does not include `kilo`. For Compose, mount the host CLI + auth yourself once — same manual step as other connectors. See [Docker Compose (host CLIs)](README.md#docker-compose-host-clis).

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `kilo run --format json --auto` |
| Session ID capture | Defensive extraction from session events, persisted to run |
| Model selection | `--model {provider}/{model}` from agent config |
| Result parsing | `extract_result_from_text` on accumulated assistant text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as `kilo.console.*` events |
| Session resume | `--session <session_id>` on continuation runs (documented but not wired in `load_resume_session_id` yet) |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `kilo run --format json --auto "<coppice_run_prompt>"` with CWD set to the agent worktree. There is no documented `-C` / `--cwd` flag on `kilo run`, so the process CWD is the worktree.
2. `--model {provider}/{model}` is added when both `model_provider` and `model` are configured on the agent. If the stored `model` already contains a `/`, it is passed through verbatim. If only `model` is set, it is passed as-is. If only `model_provider` is set, the flag is omitted.
3. Each stdout line is parsed as JSON. The provider defensively extracts the session id from common OpenCode-style fields (`properties.sessionID`, `session.id`, etc.) and accumulates assistant text from `session.message` events whose role is `assistant`. Tool / user messages are ignored.
4. The terminal `session.idle` / `session.finished` event signals completion (stdout EOF is a backstop). Coppice extracts the JSON result contract (`AgentRunResult`) from the accumulated text using `extract_result_from_text`, which scans for the `{...}` JSON object regardless of the surrounding event envelope.
5. On cancel or timeout, the subprocess is killed.

## Live streaming (WebSocket console)

Kilo's `--format json` emits newline-delimited JSON events on stdout. Each assistant text event is forwarded to the `RunStreamHandle` in real time:

| Kilo/OpenCode event | Console event | Notes |
|---------------------|---------------|-------|
| `session.message` (role: `assistant`, text part) | `kilo.console.text` | Assistant prose; markdown payload |
| Assistant text containing the result contract | `kilo.console.result` | Emits the parsed contract once; duplicate skipped |
| Tool / user / session lifecycle events | — (ignored for display) | Not forwarded as console events |

**Recovery after server restart:** The subprocess is gone, so live reattach is not possible. The WS live endpoint replays the persisted `terminal.log` artifact as a single `Frame` message. If the run was still active, it is marked interrupted.

## Session resume

`kilo run -s <session_id>` is documented for resuming a specific session. The connector adds `--session <session_id>` when `resume_session_id` is supplied. However, the job worker's `load_resume_session_id` currently only returns session ids for `claude-code` runs, so cross-run Kilo resume is not wired through the worker yet. Cross-run continuity should use the checkpoint pattern: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## Limitations

- **Daemon compatibility unverified:** `kilo serve` / `kilo daemon` exist, but OpenCode-compatible HTTP/SSE endpoints are not confirmed by the public docs. The connector does not use them. If a later Kilo version documents a compatible API, the connector can be upgraded to the daemon path used by `opencode`.
- **Event schema unverified:** The `--format json` event shape is OpenCode-derived but not version-pinned. Event parsing is defensive and reuses the OpenCode `session.message` shape; result extraction does not depend on the exact event envelope because `extract_result_from_text` scans the accumulated assistant text for the JSON contract.
- **Live recovery:** Not possible after a server restart (subprocess is gone). The persisted `terminal.log` is replayed instead.
- **No `-C` flag:** `kilo run` has no documented working-directory flag; the connector sets the process CWD instead.

## Manual verification

CI never invokes the real Kilo CLI. To verify on a host:

1. Install and authenticate the CLI:
   ```bash
   npm install -g @kilocode/cli
   kilo --version
   kilo   # then run /connect inside the TUI and add a provider
   ```
2. Enable the connector and configure model providers in `config.toml`:
   ```toml
   [agent.connectors.kilo-code]
   enabled = true
   command = "kilo"
   model_providers = ["anthropic"]
   ```
3. Confirm listing endpoints:
   ```bash
   curl -s http://127.0.0.1:5000/api/connectors | jq
   curl -s http://127.0.0.1:5000/api/connectors/kilo-code/model-providers | jq
   curl -s http://127.0.0.1:5000/api/connectors/kilo-code/model-providers/anthropic/models | jq
   ```
   The last call shells out to `kilo models anthropic` and requires the host CLI to be installed and authenticated.
4. Create an agent with `connector=kilo-code`, `model_provider=anthropic`, and a model from the list above, then start a run and watch the live console.
