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
model_providers = ["zai-coding-plan"]

# After opencode auth login — list provider IDs with: opencode auth list
# Models are chosen per agent in the UI (fetched via opencode models <provider>)
```

No server-level `model` or `variant` in config. Host adds model provider IDs to `model_providers` after authenticating with OpenCode.

Use only in local `config.toml` for manual testing. Never in CI or the agent Docker stack.

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
2. Per run: `opencode run --attach http://127.0.0.1:{port} --format json --dir <worktree> --model <provider/model> "<prompt>"`.
3. Agent reads `.agent/context.md` and must return the result contract JSON.

**`--dir`** is the working directory the agent runs in — the ticket's git worktree (e.g. `./data/worktrees/TICKET-xxx-agent-repo/`). It must be a **real path** on the same machine as `opencode serve`, not a placeholder.

The prompt is a **positional message** at the end. `-p` is `--password` (basic auth), not prompt. Use `--format json` to get machine-readable stdout (Coppice requires this).

```bash
opencode run --attach http://127.0.0.1:4096 \
  --format json \
  --model zai-coding-plan/glm-5.1 \
  --dir "$PWD" \
  "hello"
```

## Streaming

OpenCode JSON/SSE events are normalized to terminal frames for the Live Console.

## Requirements

- `opencode` on `PATH`
- `opencode auth login` completed on the **same host** as `make server-dev`
- Host dev only — do not run inside Docker (server spawns the CLI child process)

## Future TODO

- Idle shutdown/restart of `opencode serve` when no jobs are queued or running.
- `attach_url` config to use an externally managed serve instance.
