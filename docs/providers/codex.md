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
3. Pick model provider **openai** or **azure**, then a model from the list (from `codex debug models` after login).
4. Assign the agent to a ticket and start a run.

Optional config (usually set by `enable`):

```toml
[agent.connectors.codex]
enabled = true
model_providers = ["openai", "azure"]
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
- **Agent Chat:** Supported with prompt-level write denial; Coppice passes `codex exec resume <id>` on later chat turns when a session id is stored. Resume is **best-effort** — Coppice falls back to a full transcript once if resume fails.
- **Continued tickets:** Prefer checkpoint-style `continued` runs (progress note → next run). Ticket runs may also use resume when a prior `thread_id` exists.
- **Long context:** Within a single run, Codex may compact history near the model limit. Across runs, use checkpoints ([context design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## How Coppice runs Codex (reference)

Coppice spawns `codex exec --json --dangerously-bypass-approvals-and-sandbox` with worktree `-C`, optional `-m`, and the prompt on stdin. JSONL events drive the live console; accumulated agent text is parsed for Coppice’s JSON result contract.

Models: `openai` = slugs without an `azure/` prefix; `azure` = slugs with `azure/`.

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
