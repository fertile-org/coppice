# OpenCode connector

**ID:** `opencode`  
**Status:** Implemented (M04)  
**Stream backend:** `OpenCodeStream`

Runs agents via [OpenCode](https://opencode.ai) CLI in attach mode. Coppice auto-starts `opencode serve` on the host.

## Auth (not in Coppice config)

API keys live in OpenCode, not `config.toml`:

```bash
opencode auth login
opencode auth list   # verify
```

Credentials are stored at `~/.local/share/opencode/auth.json`.

## Config

```toml
[agent]
default_connector = "opencode"

[agent.connectors.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
# run_timeout_secs = 3600   # optional; default 1800 (30 min)
model_providers = ["zai-coding-plan"]

# After opencode auth login — list provider IDs with: opencode auth list
# Models are chosen per agent in the UI (fetched via opencode models <provider>)
```

No server-level `model` or `variant` in config. Host adds model provider IDs to `model_providers` after authenticating with OpenCode.

CI and the default Compose stack stay on `mock`. Use OpenCode on a host install or after mounting the CLI into Compose (see [Docker](#docker)).

### Model provider IDs

OpenCode has no separate `--provider` flag. At run time Coppice assembles `model_provider/model` and passes it to `opencode run --model`:

| Your `opencode auth list` entry | Model provider ID | Example assembled model |
|--------------------------------|-------------------|-------------------------|
| Z.AI Coding Plan api | `zai-coding-plan` | `zai-coding-plan/glm-4.7` |
| Z.AI api | `zai` | `zai/glm-4.7` |
| Alibaba api | `alibaba` | `alibaba/<model>` |
| MiniMax Token Plan | `minimax-coding-plan` | `minimax-coding-plan/<model>` |

List available provider IDs: `opencode auth list`. List models for a provider: `opencode models zai-coding-plan`.

### Per-agent model provider and model

An agent’s `model_provider` and `model` fields are set in the UI (cascading dropdowns). Coppice assembles `{model_provider}/{model}` when invoking OpenCode. The agent must use `connector = "opencode"`, and its model provider must appear in `model_providers` — otherwise health shows `missing_config` and runs are blocked.

## Execution

1. Server starts `opencode serve` on boot (when enabled or `default_connector = "opencode"`).
2. Per run, Coppice uses the **opencode serve HTTP API** (not CLI stdout):
   - `POST /session?directory=<worktree>` — create session with model
   - `POST /session/{id}/prompt_async` — send the Coppice prompt
   - `GET /event?directory=<worktree>` — SSE stream for Live Console + idle detection
   - `GET /session/{id}/message` — fetch assistant reply and parse result contract JSON
3. Agent reads `.agent/context.md` and must return a real `done` or `blocked` contract (not placeholder text).

**`directory`** must be the ticket's git worktree as an absolute path on the same host as `opencode serve` (e.g. `/data/worktrees/TICKET-xxx-agent-repo/`).

Manual CLI test (optional):

```bash
opencode run --attach http://127.0.0.1:4096 \
  --model zai-coding-plan/glm-5.1 \
  --dir "$PWD" \
  "hello"
```

Note: in attach mode the CLI child may emit **no stdout**; Coppice reads session messages from the serve API instead.

## Live Session view

OpenCode runs use a **structured session UI** (ported from OpenCode CLI), not the xterm/ANSI Live Console. The ticket drawer shows messages, tool calls, and reasoning parts as React components. Mock connector runs still use the terminal-style Live Console.

## WebSocket protocol

Connect to `ws://<host>/ws/agent-runs/{run_id}/live` (session cookie required). Message types:

| Type | When | Payload |
|------|------|---------|
| `snapshot` | First message on reconnect when no in-memory stream | `messages`, `parts`, `sessionId` — full session state from disk or memory |
| `event` | During a live run | `event` — raw OpenCode SSE JSON (e.g. `message.part.delta`) |
| `end` | Stream closed | `status`, `reason` (optional), `recoverable` (bool) |

Mock runs still receive `frame` messages (ANSI text) instead of `snapshot`/`event`.

### `recoverable` on `end`

- `true` — run is still active; client may reconnect (e.g. brief disconnect while worker is streaming).
- `false` — terminal or unrecoverable (run finished, Coppice restarted without OpenCode session, or `opencode serve` unavailable). Do not auto-reconnect in a loop.

## Recovery after Coppice restart

While a run is active, Coppice buffers live messages in memory and periodically writes `session.snapshot.json`. If Coppice restarts mid-run:

1. On boot, orphaned active runs are marked interrupted when OpenCode reports the session missing.
2. When the UI opens the live WebSocket and the in-memory registry is empty, the server replays `session.snapshot.json` (if present), then attempts to re-attach to `GET /event` on `opencode serve` using the stored `session_id` and worktree path.
3. If re-attach fails (serve down, session gone), the server sends `end` with `recoverable: false` and an error reason.

## `session.snapshot.json` artifact

Written under `{artifacts_dir}/runs/{run_id}/session.snapshot.json` during OpenCode runs (atomic write via temp file + rename). Shape:

```json
{
  "sessionId": "ses_…",
  "messages": [ … ],
  "parts": { "msg_id": [ … ] }
}
```

Used for WebSocket snapshot replay and post-mortem inspection. Mock runs do not produce this file.

## Streaming

Coppice subscribes to `GET /event?directory=<worktree>` (SSE, auto-reconnects) and polls session messages as a fallback. Events are rendered in the Live Session view as:

- **Text** — streamed incrementally via `message.part.delta` (no full-text flash)
- **Thinking** — `reasoning` parts shown dimmed
- **Tools** — `→ read: context.md` while running, `✓ read: context.md` when done

## Context compaction

OpenCode compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** call `POST /session/{id}/compact` proactively.

### Config (`~/.config/opencode/opencode.jsonc`)

| Knob | Default | Effect |
|------|---------|--------|
| `compaction.auto` | `true` | Summarize tool-heavy history automatically when the threshold is reached |
| `compaction.reserved` | `20000` | Tokens held back from the input limit before compaction fires |

Compaction triggers when:

```text
token_count >= input_limit - reserved
```

With `reserved` at 20k, a model with a 200K input limit (e.g. `glm-5.1`) rarely compacts below ~180K tokens. Typical Coppice runs stay well under that, so compaction is uncommon unless the session accumulates many tool rounds or very large outputs.

Disable auto-compaction only for debugging (`"compaction": { "auto": false }`); production runs should leave it on.

### Coppice behavior

- **No manual `/compact`** — orchestration waits for OpenCode’s automatic guard.
- **Result contract** — after compaction, Coppice still parses the final assistant `text` part (and scans `compaction` parts as a fallback). See `fixtures/opencode-events/compacted-done.jsonl`.
- **Run timeout** — Coppice waits up to **`run_timeout_secs`** (default **1800**, 30 minutes) for the OpenCode session to reach `idle`. Very long shell commands may still exceed this; increase `agent.connectors.opencode.run_timeout_secs` in `config.toml`, or prefer targeted tests (`make test-unit`, `make test-smoke`, `cargo test -p coppice-server --lib`) during agent runs. For work that spans multiple sessions, return `status: "continued"` with a `progressNote` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

### SSE events (Live Session)

OpenCode emits compaction-related events on `GET /event`:

| Event | Meaning |
|-------|---------|
| `session.compacted` | Compaction finished; session history was summarized |
| `session.next.compaction.*` | Upcoming compaction signals (e.g. threshold approaching) |

Coppice forwards these as WebSocket `event` messages. The Live Session may render `compaction` message parts when present.

## Requirements

- `opencode` on `PATH`
- `opencode auth login` completed where the server process runs (host `make server-dev`, or inside Compose after you mount the CLI + auth)

## Docker

The server image does not include `opencode`. Mount the host CLI + auth yourself once (overview: [Docker Compose (host CLIs)](README.md#docker-compose-host-clis)). OpenCode also needs `opencode serve` reachable from the server (attach mode).

**1. Host prep**

```bash
opencode auth login
opencode auth list
```

**2. Enable in** `deploy/config/config.toml`

```toml
[agent.connectors.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
model_providers = ["zai-coding-plan"]  # IDs from `opencode auth list`
```

**3. Compose override** — save as `deploy/docker-compose.opencode.yml` (local only). Adjust the binary path to match `which opencode` (often `~/.opencode/bin/opencode`):

```yaml
services:
  server:
    environment:
      HOME: ${HOME}
      PATH: ${HOME}/.opencode/bin:${HOME}/.local/bin:/usr/local/bin:/usr/bin:/bin
    volumes:
      - ${HOME}/.opencode:${HOME}/.opencode:ro
      - ${HOME}/.local/share/opencode:${HOME}/.local/share/opencode:ro
```

**4. Apply**

```bash
docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.opencode.yml \
  up -d --force-recreate server

docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.opencode.yml \
  exec -u "$(id -u):$(id -g)" server opencode --version
```

## Future TODO

- Idle shutdown/restart of `opencode serve` when no jobs are queued or running.
- `attach_url` config to use an externally managed serve instance.
