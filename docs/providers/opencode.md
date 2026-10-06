# OpenCode

Use [OpenCode](https://opencode.ai) as a Coppice connector. Coppice starts a dedicated `opencode serve` for each run and drives it through OpenCode’s HTTP/SSE API. The ticket drawer shows a **Live Session** (messages and tools), not a raw terminal.

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
| recreate server | Needed after `coppice connector enable` or a hand-edit. The in-app Connectors switch and saving an agent apply immediately. OpenCode is registered only when `enabled = true` |
| `install` | Installs `opencode` into `/home/coppice/.opencode/bin` |
| `setup` | Runs `opencode auth login` |
| `doctor` | Prints `doctor: ok` when binary + auth look healthy |

CLI binaries and auth live in the Compose volume at `/home/coppice`. You do **not** need to mount host home directories.

After login, put provider IDs into config (example):

```toml
[agent.connectors.opencode]
enabled = true
command = "opencode"
serve_hostname = "127.0.0.1"   # per-run servers listen here on a free port
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

Start at **Tools → Connectors** (admin): it shows whether `opencode` is found, whether auth is detected or verified by the `opencode auth list` probe, the probe output, the last real run, and **Test connection** runs a real gateway check with a failure reason ([diagnostics](README.md#diagnostics-tools--connectors)).

| Symptom | What to try |
|---------|-------------|
| Binary missing | Re-run `install`; PATH should include `/home/coppice/.opencode/bin` |
| Auth missing | Re-run `setup` (`opencode auth login`) |
| Agent health `missing_config` | If the message says OpenCode is turned off, turn it on in Tools → Connectors or save the agent again. If it names a model provider, add that id to `model_providers` (a hand-edit is read on the next server start) |
| Live Session empty / run fails | Check the run error (a per-run `opencode serve` that fails to start names the cause) and `doctor` |
| Run times out on long tests | Raise `run_timeout_secs`, or prefer shorter agent test commands |

## Behavior notes

- **Live Session:** Structured UI (messages, tools, reasoning), not the mock/xterm console.
- **Restart mid-run:** The per-run `opencode serve` does not survive a Coppice restart, so active OpenCode runs are marked interrupted; the Live Session replays the stored snapshot and ends non-recoverable. Chat and resume still continue the stored session id — OpenCode keeps sessions in its shared data dir.
- **Agent Chat:** Supported; later turns reuse the same OpenCode HTTP session (`prompt_async` on the stored session id) with a slim Coppice context file. No hard read-only tool allowlist — rely on chat rules and operator trust.
- **Long context:** OpenCode can compact history within a single run. Across runs, prefer `continued` checkpoints ([context design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).
- **CI / default Compose:** Stay on `mock` unless you deliberately enable OpenCode.

## How Coppice runs OpenCode (reference)

Each run (and each chat turn) spawns `opencode serve --hostname <serve_hostname> --port <free port>` with `OPENCODE_CONFIG=<artifacts_dir>/runs/<run id>/opencode.json` (runs without a gateway token, such as drafts, use a temp file). That file registers the Coppice MCP gateway as the remote server `coppice` with header `Authorization: Bearer {env:COPPICE_MCP_TOKEN}`; the token is only in the process environment, never on disk. `OPENCODE_CONFIG` merges with the global OpenCode config, so your models and auth still apply. The process is killed when the run finishes, fails, or is cancelled, and on server shutdown. `serve_port` is ignored (kept so older config files still parse).

Against that process, each run uses:

- `POST /session?directory=<worktree>`
- `POST /session/{id}/prompt_async`
- `GET /event?directory=<worktree>` (SSE → Live Session)
- `GET /session/{id}/message` (parse result contract)

`directory` must be the ticket worktree absolute path on the same host as the server (e.g. `/data/worktrees/...`).

Optional manual check (against your own `opencode serve --port 4096`):

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

- `attach_url` to use an externally managed serve instance

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
