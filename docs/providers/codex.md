# Codex

Use the [OpenAI Codex CLI](https://github.com/openai/codex) (`codex`) as a Coppice connector. Coppice starts `codex exec` for each ticket run and shows live progress in the ticket drawer.

**Connector id:** `codex`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- A Codex login (device auth) **or** the API key env your Codex install expects

## One-time setup (Docker Compose)

From the repo root, run these on the **server** container (not `web`):

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable codex
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install codex
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup codex
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor codex
```

| Step | Notes |
|------|--------|
| `enable` | Writes `enabled = true` into `deploy/config/config.toml` |
| recreate server | Needed after `coppice connector enable` or a hand-edit. The in-app Connectors switch and saving an agent apply immediately |
| `install` | May still print manual steps — put `codex` on PATH under `/home/coppice` if install is not automated yet |
| `setup` | Runs `codex login --device-auth` (follow the device-code / URL prompts) |
| `doctor` | Prints `doctor: ok` when the CLI and auth look healthy |

CLI binaries and auth live in the Compose volume at `/home/coppice`. You do **not** need to mount host home directories.

### Host install (no Docker)

```bash
coppice connector enable codex
# restart coppice-server so it reloads config
coppice connector install codex   # or install `codex` onto PATH yourself
coppice connector setup codex
coppice connector doctor codex
```

## Use it in the UI

1. Open **Agents** and create or edit an agent.
2. Set connector to **codex**.
3. Leave the model on Codex's default, or pick a model from the list (provider names are headings when there is more than one). Refresh models loads that list again.
4. Assign the agent to a ticket and start a run.

Optional config (usually set by `enable`):

```toml
[agent.connectors.codex]
enabled = true
# Optional. A non-empty list replaces the built-in provider (openai).
# model_providers = ["openai", "azure"]
# run_timeout_secs = 600
```

## If something goes wrong

Start at **Tools → Connectors** (admin): it shows whether `codex` is found, which auth is detected, the probe output, the last real run, and **Test connection** runs a real gateway check with a failure reason ([diagnostics](README.md#diagnostics-tools--connectors)).

| Symptom | What to try |
|---------|-------------|
| Binary missing | Install `codex` into `/home/coppice/.local/bin` (or your host PATH) |
| Auth missing | Re-run `setup` (`codex login --device-auth`) |
| No models in UI | Confirm login, then check `doctor` |
| Config ignored | `coppice connector enable` and hand-edits apply on the next server start. The in-app switch does not need a restart |

## Behavior notes

- **Live console:** Streams Codex output while the run is active. After a server restart mid-run, Coppice replays the saved log.
- **Agent Chat:** On, only with `--sandbox read-only`, never `--dangerously-bypass-approvals-and-sandbox`. `codex exec --sandbox read-only` is a real flag (`codex-cli 0.162.1` lists it, and `exec` defaults to a read-only sandbox). A write prompt with that flag on this machine exited `401 Unauthorized` before any tool call, so it was not shown to block a write. Chat therefore runs in a temporary local clone of the repo (`git clone --local --no-hardlinks`, every remote removed), and the chat shows: "Coppice hasn't confirmed that Codex stays read-only, so chat runs in a temporary copy of your repo. The agent can still run commands on your computer." Ticket runs still pass `--dangerously-bypass-approvals-and-sandbox`.
- **Continued tickets:** Prefer checkpoint-style `continued` runs (progress note → next run). Ticket runs may also use resume when a prior `thread_id` exists.
- **Long context:** Within a single run, Codex may compact history near the model limit. Across runs, use checkpoints ([context design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## How Coppice runs Codex (reference)

Coppice spawns `codex --no-daemon exec --json --dangerously-bypass-approvals-and-sandbox -C <worktree>` when `codex --version` is at least 0.156. Older or unknown versions omit `--no-daemon`. The prompt is on stdin. Optional `-m`, MCP `-c` overrides, and `resume` follow the pinned flags. The process cwd stays the server cwd. A result ends the run even if the process is still alive.

Models: `openai` = slugs without an `azure/` prefix; `azure` = slugs with `azure/`.

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
