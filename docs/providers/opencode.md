# OpenCode provider

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
default_provider = "opencode"

[agent.providers.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
# model = "zai-coding-plan/glm-4.7"
# variant = "high"   # optional; provider-specific reasoning effort
```

### Provider + model format

OpenCode has no separate `--provider` flag. Coppice passes `model` to `opencode run --model`, which must be **`provider_id/model_id`**:

| Your `opencode auth list` entry | Provider ID | Example `model` |
|--------------------------------|-------------|-----------------|
| Z.AI Coding Plan api | `zai-coding-plan` | `zai-coding-plan/glm-4.7` |
| Z.AI api | `zai` | `zai/glm-4.7` |
| Alibaba api | `alibaba` | `alibaba/<model>` |
| MiniMax Token Plan | `minimax-coding-plan` | `minimax-coding-plan/<model>` |

List available IDs: `opencode models` (look for `zai-coding-plan/...` lines).

Use only in local `config.toml` for manual testing. Never in CI or the agent Docker stack.

### Per-agent model override

An agent’s `model` field overrides the server default from `config.toml` for that agent’s runs. If the agent has no `model`, Coppice uses `[agent.providers.opencode] model` when set. The agent must still use `provider = "opencode"` and the server must have OpenCode enabled — otherwise health shows `missing_config` and runs are blocked.

## Execution

1. Server starts `opencode serve` on boot (when enabled or `default_provider = "opencode"`).
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
