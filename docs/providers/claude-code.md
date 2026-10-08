# Claude Code

Use the [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`) as a Coppice connector. Coppice starts `claude -p` for each ticket run and shows live progress in the ticket drawer.

**Connector id:** `claude-code`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- An Anthropic subscription login **or** an `ANTHROPIC_API_KEY`

## One-time setup (Docker Compose)

From the repo root, run these on the **server** container (not `web`):

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable claude-code
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install claude-code
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup claude-code
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor claude-code
```

| Step | Notes |
|------|--------|
| `enable` | Writes `enabled = true` into `deploy/config/config.toml` (and default model providers when missing) |
| recreate server | Needed after `coppice connector enable` or a hand-edit. The in-app Connectors switch and saving an agent apply immediately |
| `install` | May still print manual steps — put the `claude` binary on PATH under `/home/coppice` if install is not automated yet |
| `setup` | Prefers `ANTHROPIC_API_KEY` or `claude setup-token` (paste). Browser OAuth is unreliable in containers |
| `doctor` | Prints `doctor: ok` when the CLI and auth look healthy |

CLI binaries and auth live in the Compose volume at `/home/coppice`. You do **not** need to mount host home directories.

### Host install (no Docker)

```bash
coppice connector enable claude-code
# restart coppice-server so it reloads config
coppice connector install claude-code   # or install `claude` onto PATH yourself
coppice connector setup claude-code
coppice connector doctor claude-code
```

## Use it in the UI

1. Open **Agents** and create or edit an agent.
2. Set connector to **claude-code**.
3. Leave the model on Claude Code's default, or pick a model provider (`sonnet`, `opus`, or `haiku`) and a model.
4. Assign the agent to a ticket and start a run.

Optional config (usually set by `enable`):

```toml
[agent.connectors.claude-code]
enabled = true
# Optional. A non-empty list replaces the built-in providers (sonnet, opus, haiku).
# model_providers = ["sonnet", "opus", "haiku"]
# run_timeout_secs = 600
```

## If something goes wrong

Start at **Tools → Connectors** (admin): it shows whether `claude` is found, which auth is detected, the probe output, the last real run, and **Test connection** runs a real gateway check with a failure reason ([diagnostics](README.md#diagnostics-tools--connectors)).

| Symptom | What to try |
|---------|-------------|
| Binary missing | Install `claude` into `/home/coppice/.local/bin` (or your host PATH) |
| Auth missing | Set `ANTHROPIC_API_KEY` on the server, or re-run `setup` / `setup-token` |
| Config ignored | `coppice connector enable` and hand-edits apply on the next server start. The in-app switch does not need a restart |

## Behavior notes

- **Live console:** Streams Claude Code output while the run is active. After a server restart mid-run, Coppice replays the saved log.
- **Agent Chat:** Later turns pass `--resume` with a narrow read-only `--allowedTools` set; Coppice stores the session id on the chat session row.
- **Continued tickets:** Follow-up runs can resume the same Claude session when Coppice has a prior `session_id`.
- **Long context:** Within a single run, Claude Code may compact history near the model limit. Across runs, prefer checkpoint-style `continued` results (see [context design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

## How Coppice runs Claude Code (reference)

Coppice spawns `claude -p "<prompt>" --output-format stream-json --verbose --permission-mode bypassPermissions` with the worktree as the process cwd, then optional `--allowedTools`, `--model`, `--resume`, and per-run MCP flags. Those pinned flags live on the connector descriptor. Stdout NDJSON is mapped to live frames; the final result text is parsed for Coppice’s JSON result contract. A result ends the run even if `claude` is still waiting on a background command.

More: [providers README](README.md), [M08](../milestones/M08-connector-operator-cli.md).
