# Cursor CLI Connector Design

**Status:** Draft — awaiting user review  
**Date:** 2026-08-05  
**Topic:** Add a `cursor` agent connector that drives the Cursor Agent CLI as a subprocess, at Claude Code parity.

## Decision summary

Coppice will add a dedicated `cursor` connector that spawns the Cursor Agent CLI (`agent` by default) in non-interactive print mode, the same way `claude-code` and `codex` use their CLIs. Orchestration stays connector-agnostic. Auth is host-managed only: the operator runs `agent login` on the machine that hosts the Coppice server; Coppice does not store, inject, or require API keys.

v1 targets Claude Code parity: stream-json live console, early `session_id` capture, and job-worker `--resume` wiring. Implementation is a dedicated provider module (not a shared stream-json base with Claude Code, and not the Cursor TypeScript/Python SDK).

## Goals

- Run Coppice agent jobs through Cursor Agent CLI in the existing worktree + result-contract pipeline.
- Surface live console frames over the existing WebSocket run stream.
- Resume prior Cursor chat sessions across `continued` runs when a `session_id` was persisted.
- List models live from the installed CLI for the Agents UI, under a single synthetic model provider `cursor`.

## Non-goals (v1)

- Cursor SDK (`@cursor/sdk` / `cursor-sdk`) or cloud / private worker integrations.
- App-managed `CURSOR_API_KEY` or any credential vault in Coppice config.
- Cursor-native `--worktree` / `-w` (Coppice already owns worktrees).
- Shared subprocess/stream-json abstraction extracted from `claude-code`.
- MCP injection into Cursor (same follow-up posture as other connectors).
- Character-level `--stream-partial-output` streaming (optional later).
- Explicit `--sandbox` policy UI (default: rely on `--force` + host CLI config).

## Background: Cursor CLI surface

Observed on Cursor Agent CLI `2026.07.23` (binary `agent` → `cursor-agent`):

| Concern | Mechanism |
|---------|-----------|
| Non-interactive | `-p` / `--print` |
| Structured stream | `--output-format stream-json` (NDJSON) |
| Unattended approvals | `--force` (alias `--yolo`) |
| Workspace trust | `--trust` (required headless; otherwise prompts / fails) |
| Workspace path | `--workspace <path>` |
| Model | `--model <id>` |
| Resume | `--resume <chatId>` |
| Auth | `agent login` (host); optional host env `CURSOR_API_KEY` is a CLI feature Coppice does not manage |
| Models list | `agent models` / `--list-models` (`id - label` lines) |
| Session id | Present on stream events as `session_id` |
| Terminal event | `type: "result"` with `result` text and `subtype` / `is_error` |

Stream event types observed: `system` (subtype `init`), `user`, `thinking`, `assistant`, `tool_call` (subtypes `started` / `completed` with nested `*ToolCall` payloads such as `editToolCall`), `result`.

This shape is closer to Claude Code (`claude -p --output-format stream-json`) than to Codex (`codex exec --json`).

## Architecture

```
Job worker
  → ConnectorRegistry.get("cursor")
  → CursorProvider.run(AgentRunInput)
      → spawn configured command with fixed Coppice flags
      → NDJSON stdout → CursorConsolePublisher → RunStreamHandle
      → session_id → session_created_tx (persisted on agent run)
      → type=result → extract_result_from_text → AgentRunResult
```

Coppice never starts Cursor’s own worktree helper. The CLI runs against the Coppice-managed worktree via `--workspace` (and process CWD set to that worktree for consistency).

## Configuration

```toml
[agent.connectors.cursor]
enabled = true
command = "agent"          # overridable (e.g. "cursor-agent")
# run_timeout_secs = 600
model_providers = ["cursor"]
```

| Field | Default | Notes |
|-------|---------|-------|
| `enabled` | `false` | Must be true to register the connector |
| `command` | `"agent"` | PATH binary; operators may set `cursor-agent` |
| `run_timeout_secs` | `600` | Kill subprocess on timeout |
| `model_providers` | `[]` | Operators add `"cursor"` after host login |

Connector id in APIs and agent records: `cursor`.

## Auth

Host-managed, identical policy to `claude-code` / `codex` / `kilo-code`:

1. Operator installs Cursor Agent CLI on the host that runs `coppice-server`.
2. Operator runs `agent login` (or otherwise establishes CLI auth on that host).
3. Coppice spawns the child with the server’s inherited environment.
4. Coppice does not read, write, validate, or document app-level API keys as part of this connector’s setup path.

If the CLI is missing or not authenticated, runs fail at spawn or with a CLI error result; health/liveness follows existing connector patterns.

## Invocation

Fixed flags for every Coppice-driven run:

```text
{command} -p --trust --force --output-format stream-json
  --workspace <coppice-worktree>
  [--model <model>]
  [--resume <session_id>]
  <coppice_run_prompt>
```

- Prompt: existing `coppice_run_prompt()` (same contract as other connectors).
- Do **not** pass Cursor `-w` / `--worktree`.
- Do **not** use interactive modes or omit `--trust` / `--force` in the worker path.

## Components

| Component | Responsibility |
|-----------|----------------|
| `CursorConnectorConfig` in `config/` | Deserialize connector settings |
| `providers/cursor.rs` | `AgentProvider` — spawn, stream loop, cancel, timeout, result parse |
| `providers/cursor_console.rs` | Map NDJSON events to live console frames |
| `providers/cursor_models.rs` | Invoke `{command} models`, parse available models |
| `ConnectorRegistry` | Register when enabled; return `["cursor"]` model providers from config |
| Job worker | Enable `session_created_tx` for `cursor`; `load_resume_session_id` for `cursor` like `claude-code` |
| WS live recovery | Treat `cursor` as subprocess-backed (replay `terminal.log`; no live reattach) |
| `docs/providers/cursor.md` | Operator guide (auth, config, capabilities, limitations) |

## Live console mapping

| Cursor event | Behavior |
|--------------|----------|
| `system` / `init` | Session start frame; capture `session_id` via `session_created_tx` if not yet sent |
| `assistant` | Frame with assistant text from `message.content[].text` |
| `tool_call` started/completed | Short summary frames; defensively read nested tool payloads (`editToolCall`, shell/read variants, paths when present) |
| `thinking` | Ignore |
| `user` | Ignore |
| `result` | Emit result card once; use `result` string as contract source |

Unknown fields and unknown tool shapes are ignored without failing the run. Field additions from future CLI versions must be tolerated.

## Result parsing and errors

1. Prefer the terminal `type: "result"` payload’s `result` string.
2. Run existing `extract_result_from_text` to obtain `AgentRunResult` (`done` / `blocked` / `continued`, etc.).
3. If `is_error` is true or `subtype` indicates failure, fail the provider run with a clear error (do not invent a success contract).
4. If no contract JSON is found in the result text, fail like other CLI connectors (no silent empty success).
5. Cancel via `cancel_rx` kills the child → `ProviderError::Cancelled`.
6. Timeout kills the child → timeout error including configured seconds.
7. Spawn failures (missing binary) surface as I/O errors.

## Session resume

- Persist `session_id` from stream events onto the agent run (same persistence path as Claude Code).
- On continuation runs for connector `cursor`, `load_resume_session_id` returns the previous run’s session id and the provider passes `--resume <id>`.
- Checkpoint / `continued` + context.md resume remains available as the cross-run safety net if CLI resume fails (same operational story as other connectors).

## Models

- Single synthetic model provider id: `cursor`.
- Operators set `model_providers = ["cursor"]` in config.
- `GET /api/connectors/cursor/model-providers/cursor/models` shells out to the configured command’s models list and returns ids for the Agents UI.
- No hardcoded model catalog in Coppice. If the CLI is missing or listing fails, the models endpoint returns an error (existing pattern).

## Testing

| Layer | Expectation |
|-------|-------------|
| Unit | Fixture JSONL under `fixtures/cursor/` — session capture, console mapping, contract extraction |
| Config / registry | Enable/disable registration; `model_providers_for("cursor")` |
| CI integration / E2E | Remain on `mock` only; no live Cursor CLI |
| Manual | Host with logged-in `agent`, connector enabled, one end-to-end ticket run |

## Documentation updates

- Add `docs/providers/cursor.md` (status, auth, config, capabilities, stream mapping, resume, limitations).
- Link from `docs/providers/README.md` and `docs/providers.md`.
- Note Docker/default-stack caveat: the CLI must be available inside the environment that executes `coppice-server` (PATH + host login state), same class of constraint as `claude` / `codex`.

## Implementation approach

Mirror the Claude Code connector as a dedicated module set (Approach 1 from brainstorming). Do not refactor Claude Code into a shared base in this change. Wire registry, worker session/resume hooks, WS subprocess recovery list, config defaults/examples, and provider docs in the same delivery.

## Acceptance criteria

1. With `[agent.connectors.cursor] enabled = true` and a host-authenticated `agent` binary, an agent assigned `connector = "cursor"` and `modelProvider = "cursor"` can complete a run and produce a parsed `AgentRunResult`.
2. Live console shows assistant and tool summary frames during the run.
3. `session_id` is persisted; a follow-up run for the same connector passes `--resume` when a prior id exists.
4. Models endpoint lists live CLI models under provider `cursor`.
5. Automated tests cover parsing/registry without invoking the real CLI.
6. Docs describe host-managed auth only (no Coppice API key setup).
