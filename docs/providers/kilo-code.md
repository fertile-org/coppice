# Kilo Code

Use the [Kilo Code CLI](https://kilo.ai/docs/code-with-ai/platforms/cli) (`kilo`, from `@kilocode/cli`) as a Coppice connector. Coppice starts `kilo run` for each ticket run and shows live progress in the ticket drawer.

**Connector id:** `kilo-code`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- A Kilo / provider login (TUI `/connect` or `kilo auth login`)

## One-time setup (Docker Compose)

From the repo root, run these on the **server** container (not `web`):

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable kilo-code
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install kilo-code
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup kilo-code
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor kilo-code
```

| Step | Notes |
|------|--------|
| `enable` | Writes `enabled = true` into `deploy/config/config.toml` |
| recreate server | Picks up the config change |
| `install` | Usually manual — e.g. install `@kilocode/cli` so `kilo` lands under `/home/coppice` on PATH |
| `setup` | Follow vendor auth (`kilo auth login` or open `kilo` and use `/connect`) |
| `doctor` | Prints `doctor: ok` when the CLI and auth look healthy |

CLI binaries and auth live in the Compose volume at `/home/coppice`. You do **not** need to mount host home directories.

### Host install (no Docker)

```bash
npm install -g @kilocode/cli
coppice connector enable kilo-code
# restart coppice-server so it reloads config
coppice connector setup kilo-code
coppice connector doctor kilo-code
```

## Use it in the UI

1. Open **Agents** and create or edit an agent.
2. Set connector to **kilo-code**.
3. Pick a model provider (e.g. `anthropic`) and a model from the list (from `kilo models <provider>` after login).
4. Assign the agent to a ticket and start a run.

Optional config (usually set by `enable`):

```toml
[agent.connectors.kilo-code]
enabled = true
command = "kilo"
model_providers = ["anthropic", "openai"]
# run_timeout_secs = 600
```

## If something goes wrong

| Symptom | What to try |
|---------|-------------|
| Binary missing | Install `@kilocode/cli` so `kilo` is on PATH under `/home/coppice` |
| Auth missing | Re-run `setup` or authenticate in the Kilo TUI (`/connect`) |
| No models in UI | Confirm `kilo models <provider>` works inside the server container |
| Config ignored | Recreate/restart the server after `enable` |

## Behavior notes

- **Live console:** Streams assistant output while the run is active. After a server restart mid-run, Coppice replays the saved log.
- **Continued tickets:** Prefer checkpoint-style `continued` runs. Worker-wired session resume for Kilo is not fully connected yet.
- **Daemon / serve:** Coppice uses the **subprocess** path (`kilo run`), not `kilo serve` / daemon HTTP APIs (compatibility not confirmed).

## How Coppice runs Kilo (reference)

Coppice spawns roughly:

```text
kilo run --format json --auto --model <provider>/<model> "<prompt>"
```

with CWD set to the worktree. Stdout JSON events are parsed defensively (OpenCode-derived shapes); assistant text is scanned for Coppice’s JSON result contract.

Vendor docs: [CLI](https://kilo.ai/docs/code-with-ai/platforms/cli), [CLI reference](https://kilo.ai/docs/code-with-ai/platforms/cli-reference).

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
