# Cursor

Use the [Cursor Agent CLI](https://cursor.com) (`cursor-agent`) as a Coppice connector. Coppice starts `cursor-agent` for each ticket run and shows live progress in the ticket drawer.

**Connector id:** `cursor`

## Prerequisites

- Coppice running via Docker Compose (`make compose-up`), or a host install with `coppice` on your PATH
- A Cursor account that can log in with `cursor-agent login`

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
| `install` | Downloads `cursor-agent` into the container’s home volume |
| `setup` | Runs `cursor-agent login` — copy the printed URL into a browser on your machine, finish login, return to the terminal |
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
3. Leave the model on Cursor's default, or pick one from the list. Refresh models loads that list again.
4. Assign the agent to a ticket and start a run. Live output appears in the ticket drawer.

Optional config (usually set by `enable`):

```toml
[agent.connectors.cursor]
enabled = true
command = "cursor-agent"
# Optional. A non-empty list replaces the built-in provider (cursor).
# model_providers = ["cursor"]
# run_timeout_secs = 1800
```

Older Coppice versions wrote `command = "agent"`. Grok also installs an `agent` command, so Coppice reads that old value as `cursor-agent`.

## If something goes wrong

Start at **System → Connectors** (admin): it shows whether `cursor-agent` is found, whether auth is detected or verified by the `cursor-agent models` probe, the probe output, the last real run, and **Test connection** runs a real gateway check with a failure reason ([diagnostics](README.md#diagnostics-tools--connectors)).

| Symptom | What to try |
|---------|-------------|
| `doctor` says binary missing | Re-run `install`, or confirm PATH includes `/home/coppice/.local/bin` inside the server container |
| `doctor` says auth missing / models fail | Re-run `setup` and finish the browser login |
| Agents UI has no models / API errors | Same as auth missing. Saving the agent turns the connector on; a hand-edit is read on the next server start |
| Changes to config.toml ignored | Hand-edits apply on the next server start (`docker compose … up -d --force-recreate server`). The in-app switch does not need that |

## Behavior notes

- **Live console:** Streams Cursor’s progress while the run is active. After a server restart mid-run, live reattach is not possible; Coppice replays the saved log and marks an interrupted run.
- **Agent Chat:** On, in the per-chat folder, no banner. Chat passes `--mode ask`, omits `--force`, and writes a per-run `cli-config.json` that denies `Shell(*)` and `Write(**)`. That deny is intentional: `--mode ask` blocks edit/apply but not shell, so chat and plan lose Shell entirely — including `git log` and `rg`. Use Coppice MCP tools instead. A project `.cursor/cli.json` is replaced with the same deny for the run and restored after; without that overlay a repo allow list replaces `CURSOR_CONFIG_DIR` permissions. Live probe on `cursor-agent` 2026.10.22-27dcf5f (`server/tests/real_cursor.rs`): no worktree or absolute-path write, no repo MCP start. Ask mode called `result_submit` and it was accepted. Cursor staff say ask mode does not hard-block shell writes unless `--sandbox enabled`, and that sandbox also blocks network and MCP; Coppice does not pass `--sandbox` (that is M13). Ticket runs still pass `--force` and write an empty deny list. Plan runs use `--mode plan` and the same deny. Plan and other read-only ticket runs use `runs/<run id>` for `CURSOR_CONFIG_DIR` so they cannot overwrite a concurrent write run's `cli-config.json` on the same ticket. Work resume is unchanged: plan does not persist a session id.
- **Continued tickets:** Follow-up runs can resume the same Cursor chat session when an earlier Cursor run on the ticket left a `session_id`. Sessions from another connector are skipped, so switching an agent to Cursor starts a new session.
- **Worktrees:** Coppice owns git worktrees. It does not pass Cursor’s `-w` / `--worktree` flag.
- **MCP:** For the run, Coppice writes `<worktree>/.cursor/mcp.json` and a per-run `CURSOR_CONFIG_DIR` allow rule for `Mcp(coppice:*)`. `HOME` is not remapped, so `cursor-agent login` / Keychain still work. The worktree file is restored when the run ends. Write-capable runs merge any existing repo servers into that file. Read-only runs replace it with only the Coppice gateway so `--approve-mcps` cannot start a repo stdio server that writes. Read-only runs also replace `<worktree>/.cursor/cli.json` with the same deny/allow for the run and restore it after: a project allow list in that file replaces `CURSOR_CONFIG_DIR` permissions and would otherwise drop the deny (and reject Coppice MCP as "User rejected MCP"). MCP runs pass `--approve-mcps`: without it `-p` leaves the server on "needs approval" (`--force` does not imply it). Read-only runs without MCP access fail closed (`mcp_unavailable`). `$HOME/.cursor/mcp.json` can still be visible because `HOME` is real.

## How Coppice runs Cursor (reference)

**Ticket / write-capable runs** spawn roughly:

```text
cursor-agent -p "<prompt>" --trust --force --output-format stream-json --workspace <worktree>
```

Plan runs use `--mode plan` and omit `--force`. Ask mode is the chat flag. Neither mode was shown to block shell writes; read-only launches add the `Shell(*)` / `Write(**)` deny in `cli-config.json`. Stdout is NDJSON (`stream-json`); the final `result` event is parsed for Coppice’s JSON result contract. Live events are published as `cursor.console.*` on the run WebSocket. Ticket runs accept optional `--model` and `--resume <session_id>`.

More on connectors in general: [providers README](README.md). Milestone notes: [M08](../milestones/M08-connector-operator-cli.md).
