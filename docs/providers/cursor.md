# Cursor

Use the [Cursor Agent CLI](https://cursor.com) (`agent`) as a Coppice connector. Coppice starts `agent` for each ticket run and shows live progress in the ticket drawer.

**Connector id:** `cursor`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- A Cursor account that can log in with `agent login`

## One-time setup (Docker Compose)

From the repo root, run these on the **server** container (not `web`):

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable cursor
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor cursor
```

What each step does:

| Step | What you should see |
|------|---------------------|
| `enable` | Writes `enabled = true` into `deploy/config/config.toml` |
| recreate server | Needed after `coppice connector enable` or a hand-edit. The in-app Connectors switch and saving an agent apply immediately |
| `install` | Downloads `agent` into the container’s home volume |
| `setup` | Runs `agent login` — copy the printed URL into a browser on your machine, finish login, return to the terminal |
| `doctor` | Prints `doctor: ok` when the CLI and login look healthy |

CLI binaries and login state live in a Compose volume at `/home/coppice`. You do **not** need to mount host `~/.local` or `~/.config`.

### Host install (no Docker)

```bash
coppice connector enable cursor
# restart your coppice-server process so it reloads config
coppice connector install cursor   # or install Cursor Agent CLI yourself onto PATH
coppice connector setup cursor
coppice connector doctor cursor
```

## Use it in the UI

1. Open **Agents** and create or edit an agent.
2. Set connector to **cursor**.
3. Pick model provider **cursor** and a model from the list (loaded from `agent models` after login).
4. Assign the agent to a ticket and start a run. Live output appears in the ticket drawer.

Optional config (usually set by `enable`):

```toml
[agent.connectors.cursor]
enabled = true
command = "agent"
model_providers = ["cursor"]
# run_timeout_secs = 600
```

## If something goes wrong

Start at **Tools → Connectors** (admin): it shows whether `agent` is found, whether auth is detected or verified by the `agent models` probe, the probe output, the last real run, and **Test connection** runs a real gateway check with a failure reason ([diagnostics](README.md#diagnostics-tools--connectors)).

| Symptom | What to try |
|---------|-------------|
| `doctor` says binary missing | Re-run `install`, or confirm PATH includes `/home/coppice/.local/bin` inside the server container |
| `doctor` says auth missing / models fail | Re-run `setup` and finish the browser login |
| Agents UI has no models / API errors | Same as auth missing. Saving the agent turns the connector on; a hand-edit is read on the next server start |
| Changes to config.toml ignored | Hand-edits apply on the next server start (`docker compose … up -d --force-recreate server`). The in-app switch does not need that |

## Behavior notes

- **Live console:** Streams Cursor’s progress while the run is active. After a server restart mid-run, live reattach is not possible; Coppice replays the saved log and marks an interrupted run.
- **Agent Chat:** Later turns use `--resume` with `--mode ask` (read-only); Coppice stores the session id on the chat session row.
- **Continued tickets:** Follow-up runs can resume the same Cursor chat session when Coppice has a prior `session_id`.
- **Worktrees:** Coppice owns git worktrees. It does not pass Cursor’s `-w` / `--worktree` flag.
- **MCP:** Not injected by Coppice in this version.

## How Coppice runs Cursor (reference)

**Ticket / write-capable runs** spawn roughly:

```text
agent -p "<prompt>" --trust --force --output-format stream-json --workspace <worktree>
```

**Agent Chat turns** use Cursor’s read-only Q&A mode (no `--force`):

```text
agent -p "<prompt>" --trust --mode ask --output-format stream-json --workspace <worktree>
```

Both paths accept optional `--model` and `--resume <session_id>`. Stdout is NDJSON (`stream-json`); the final `result` event is parsed for Coppice’s JSON result contract. Live events are published as `cursor.console.*` on the run WebSocket.

More on connectors in general: [providers README](README.md). Milestone notes: [M08](../milestones/M08-connector-operator-cli.md).
