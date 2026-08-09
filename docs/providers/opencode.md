# OpenCode

Use [OpenCode](https://opencode.ai) as a Coppice connector. Coppice starts `opencode serve` and drives each ticket run through OpenCode’s HTTP/SSE API. The ticket drawer shows a **Live Session** (messages and tools), not a raw terminal.

**Connector id:** `opencode`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- An OpenCode login (`opencode auth login`) for the providers you want

## One-time setup (Docker Compose)

From the repo root, run these on the **server** container (not `web`):

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable opencode
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor opencode
```

| Step | Notes |
|------|--------|
| `enable` | Turns the connector on in `deploy/config/config.toml` — also set `model_providers` to IDs from `opencode auth list` (see below) |
| recreate server | Picks up config; starts `opencode serve` when OpenCode is enabled |
| `install` | Installs `opencode` into `/home/coppice/.opencode/bin` |
| `setup` | Runs `opencode auth login` |
| `doctor` | Prints `doctor: ok` when binary + auth look healthy |

CLI binaries and auth live in the Compose volume at `/home/coppice`. You do **not** need to mount host home directories.

After login, put provider IDs into config (example):

```toml
[agent.connectors.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
model_providers = ["zai-coding-plan"]
# run_timeout_secs = 3600   # optional; default 1800 (30 min)
```

### Host install (no Docker)

```bash
coppice connector enable opencode
# edit config.toml model_providers, then restart coppice-server
coppice connector install opencode   # or install OpenCode onto PATH yourself
coppice connector setup opencode
coppice connector doctor opencode
```

## Use it in the UI

1. Open **Agents** and create or edit an agent.
2. Set connector to **opencode**.
3. Pick a model provider (must be listed in `model_providers`) and a model.
4. Assign the agent to a ticket and start a run. Watch the **Live Session** in the ticket drawer.

OpenCode has no separate `--provider` flag. Coppice sends `model_provider/model` to OpenCode. Common IDs after `opencode auth list`:

| Your `opencode auth list` entry | Model provider ID | Example |
|--------------------------------|-------------------|---------|
| Z.AI Coding Plan api | `zai-coding-plan` | `zai-coding-plan/glm-4.7` |
| Z.AI api | `zai` | `zai/glm-4.7` |
| Alibaba api | `alibaba` | `alibaba/<model>` |
| MiniMax Token Plan | `minimax-coding-plan` | `minimax-coding-plan/<model>` |

List models: `opencode models zai-coding-plan` (inside the server container or on the host where OpenCode is installed).

## If something goes wrong

| Symptom | What to try |
|---------|-------------|
| Binary missing | Re-run `install`; PATH should include `/home/coppice/.opencode/bin` |
| Auth missing | Re-run `setup` (`opencode auth login`) |
| Agent health `missing_config` | Add the agent’s model provider id to `model_providers`, recreate server |
| Live Session empty / run fails | Confirm `opencode serve` is up (enabled connector + recreate); check `doctor` |
| Run times out on long tests | Raise `run_timeout_secs`, or prefer shorter agent test commands |

## Behavior notes

- **Live Session:** Structured UI (messages, tools, reasoning), not the mock/xterm console.
- **Restart mid-run:** Coppice may replay a session snapshot and try to re-attach to `opencode serve`. If serve or the session is gone, the UI gets a non-recoverable end.
- **Long context:** OpenCode can compact history within a single run. Across runs, prefer `continued` checkpoints ([context design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).
- **CI / default Compose:** Stay on `mock` unless you deliberately enable OpenCode.

## How Coppice runs OpenCode (reference)

When enabled, the server starts `opencode serve` and per run uses:

- `POST /session?directory=<worktree>`
- `POST /session/{id}/prompt_async`
- `GET /event?directory=<worktree>` (SSE → Live Session)
- `GET /session/{id}/message` (parse result contract)

`directory` must be the ticket worktree absolute path on the same host as serve (e.g. `/data/worktrees/...`).

Optional manual check:

```bash
opencode run --attach http://127.0.0.1:4096 \
  --model zai-coding-plan/glm-5.1 \
  --dir "$PWD" \
  "hello"
```

### WebSocket (Live Session)

`ws://<host>/ws/agent-runs/{run_id}/live` (session cookie required): `snapshot`, `event` (OpenCode SSE JSON), `end` (`recoverable` true/false). Mock runs still use `frame` messages.

### Context compaction knobs

In OpenCode’s own config (`~/.config/opencode/opencode.jsonc` under the managed home in Compose):

| Knob | Default | Effect |
|------|---------|--------|
| `compaction.auto` | `true` | Auto-summarize when near the input limit |
| `compaction.reserved` | `20000` | Tokens held back before compaction |

Coppice does not call compact APIs itself. Leave auto-compaction on for normal use.

### Future

- Idle shutdown/restart of `opencode serve` when no jobs are running
- `attach_url` to use an externally managed serve instance

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
