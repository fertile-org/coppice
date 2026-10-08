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
| recreate server | Needed after `coppice connector enable` or a hand-edit. The in-app Connectors switch and saving an agent apply immediately |
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
3. Leave the model on Kilo Code's default, or pick a provider and model. With no `model_providers` override, Coppice reads providers from `kilo models` (a few seconds; if that fails, only the default is offered).
4. Assign the agent to a ticket and start a run.

Optional config (usually set by `enable`):

```toml
[agent.connectors.kilo-code]
enabled = true
command = "kilo"
# Optional. A non-empty list replaces the providers read from `kilo models`.
# model_providers = ["anthropic", "openai"]
# run_timeout_secs = 600
```

## If something goes wrong

Start at **Tools → Connectors** (admin): it shows whether `kilo` is found, which auth is detected, the probe output, the last real run, and **Test connection** runs `kilo --version` and reports whether that command succeeded ([diagnostics](README.md#diagnostics-tools--connectors)). A ticket run failing with `mcp_unavailable` suggests the CLI ignores `KILO_CONFIG` (see the [providers README](README.md#coppice-mcp-gateway-tool-first-runs)).

| Symptom | What to try |
|---------|-------------|
| Binary missing | Install `@kilocode/cli` so `kilo` is on PATH under `/home/coppice` |
| Auth missing | Re-run `setup` or authenticate in the Kilo TUI (`/connect`) |
| No models in UI | Confirm `kilo models <provider>` works inside the server container |
| Config ignored | `coppice connector enable` and hand-edits apply on the next server start. The in-app switch does not need a restart |

## Behavior notes

- **Sign-in:** Kilo has no cheap non-interactive auth check (`kilo auth` opens a TUI). A found binary is labeled **Found (sign-in not checked)**, not Ready or "Found, not signed in".
- **Live console:** Streams assistant output while the run is active. After a server restart mid-run, Coppice replays the saved log.
- **Continued tickets:** Prefer checkpoint-style `continued` runs. Worker-wired session resume for Kilo is not fully connected yet.
- **Daemon / serve:** Coppice uses the **subprocess** path (`kilo run`), not `kilo serve` / daemon HTTP APIs (compatibility not confirmed).

## How Coppice runs Kilo (reference)

Coppice spawns roughly:

```text
kilo run --format json --auto --dir <worktree> "<prompt>" --model <provider>/<model>
```

with the process cwd set to the worktree. Stdout JSON events are parsed defensively (OpenCode-derived shapes); assistant text is scanned for Coppice’s JSON result contract. A result ends the run even if the process is still alive.

Vendor docs: [CLI](https://kilo.ai/docs/code-with-ai/platforms/cli), [CLI reference](https://kilo.ai/docs/code-with-ai/platforms/cli-reference).

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
