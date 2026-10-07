# Connectors: developer guide

This page is for building Coppice from source, running it in Docker, or adding a connector. Using the desktop app? Start with [providers.md](../providers.md).

A connector is how Coppice drives one agent CLI. The board, tickets and worktrees work the same for every connector; each agent picks one.

**Source of truth.** [`connectors/src/lib.rs`](../../connectors/src/lib.rs) holds one descriptor per connector: id, display name, binary, sign-in hints, probe, MCP wiring, console kind and capabilities. The server, the `coppice connector` CLI and the web app (through `GET /api/connectors`) all read it. If this page and the descriptor disagree, the descriptor wins.

## The connectors

| Connector | id | Command | How Coppice runs it | Sign-in check | Guide |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `claude-code` | `claude` | Subprocess in the worktree: `claude -p … --output-format stream-json` | `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN` set, or `~/.claude/.credentials.json`; otherwise `claude auth status` | [claude-code.md](claude-code.md) |
| Codex | `codex` | `codex` | Subprocess: `codex exec --json` | `OPENAI_API_KEY` set, or `~/.codex/auth.json`; otherwise `codex login status` | [codex.md](codex.md) |
| Cursor | `cursor` | `agent` | Subprocess: `agent -p … --output-format stream-json` | `~/.config/cursor/auth.json` or `~/.cursor/auth.json`. Neither means not signed in | [cursor.md](cursor.md) |
| OpenCode | `opencode` | `opencode` | A fresh `opencode serve` per run, driven over HTTP/SSE. The ticket shows a Live Session | `~/.local/share/opencode/auth.json`; otherwise `opencode auth list` | [opencode.md](opencode.md) |
| Kilo Code | `kilo-code` | `kilo` | Subprocess: `kilo run --format json --auto` | None. `kilo auth` is interactive, so Coppice never runs it | [kilo-code.md](kilo-code.md) |

### Detection and status

At startup, the server adds common bin dirs to its `PATH` if they exist and aren't already there: `~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `/opt/homebrew/bin` and `/usr/local/bin`. The desktop app also passes in the `PATH` from your login shell. Probes and runs use this same `PATH`.

For each connector, Coppice resolves the binary (the `command` key if set, otherwise the descriptor binary). It runs the probe (10 s cap), then the sign-in check: env var names, credential files, or the CLI's own status command (3 s cap). The sign-in check never opens a browser, prompts, or calls a model. The result is shown in the agent form and in Tools → Connectors:

| Readiness | Label | When |
| --- | --- | --- |
| `ready` | Ready | Binary found and sign-in verified |
| `found_not_signed_in` | Found, not signed in | Binary found and a reliable check says no sign-in |
| `not_on_path` | Not on your PATH | Binary not found |
| `found` | *(no label)* | Binary found, sign-in can't be checked |

Kilo Code is always `found` once `kilo` is on `PATH`. Coppice makes no sign-in claim for it.

Probes run at server start, on **Run check**, and when **Test connection** starts. They don't run on every page load.

## Configuration

Each connector has a `[agent.connectors.<id>]` section in `config.toml`:

| Key | Connectors | Default |
| --- | --- | --- |
| `enabled` | all | `false` |
| `command` | `cursor` (`agent`), `opencode` (`opencode`), `kilo-code` (`kilo`) | Claude Code and Codex always run `claude` / `codex` |
| `model_providers` | all | Empty. Turning a connector on fills an empty list from the descriptor: `claude-code` → `sonnet, opus, haiku`; `codex` → `openai`; `cursor` → `cursor`; `kilo-code` → `anthropic`; `opencode` → none (list IDs from `opencode auth list` yourself) |
| `run_timeout_secs` | all | `600`; `opencode` `1800` |
| `serve_hostname` | `opencode` | `127.0.0.1` (each run's server picks a free port; `serve_port` is ignored) |

`[agent] default_connector` sets the default for new agents. Each agent can override its connector, model provider and model on the Agents page.

Where the config lives:

| How you run Coppice | File |
| --- | --- |
| Desktop app | `config.toml` in the app data dir (`~/Library/Application Support/Coppice/` on macOS, `~/.config/Coppice/` on Linux). Only this file is read |
| From source | Defaults, then `~/.config/coppice/config.toml`, then `./config.toml`, then `COPPICE_CONFIG`, then environment variables (last wins) |
| Docker Compose | `deploy/config/config.toml`, mounted at `/etc/coppice/config.toml` |

The Tools → Connectors switch and saving an agent both turn a connector on immediately. They write into `config.toml` and keep comments and other keys. `coppice connector enable` and hand edits apply the next time the server starts.

## Chat and session resume

Agent Chat turns run with read-only tools. Coppice stores the full transcript itself. When a connector supports resume, later turns reuse the CLI session with a slim context; if resume fails, one retry sends the full transcript.

| Connector | Chat session resume | Read-only tools in chat |
| --- | --- | --- |
| `claude-code` | `--resume` | Enforced with an `--allowedTools` read-only list |
| `cursor` | `--resume` | Enforced with `--mode ask` |
| `codex` | `codex exec resume` (best effort) | Not enforced by the CLI; prompt and rules only |
| `opencode` | Reuses the OpenCode session | Not enforced by the CLI; prompt and rules only |
| `kilo-code` | Skipped | Can't be enforced, so the run is refused: ``connector `kilo-code` cannot enforce read-only tools for conversation chat turns`` |

Ticket runs can resume a prior session on `claude-code` and `cursor`. The other connectors start a fresh session and continue from the ticket checkpoint.

## Coppice MCP gateway (tool-first runs)

Every run gets a per-run token for the Coppice MCP gateway at `POST /mcp`, handed to the CLI as `COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN`. Each connector wires the gateway for that run only, through flags, process env, or a file under `<artifacts_dir>/runs/<run id>/`. Nothing is written to the worktree, a registered repo, or your global CLI config. The token is never written to a file. If a connector can't be wired, the run fails with `mcp_unavailable`.

| Connector | Mechanism | Checked against a live CLI |
| --- | --- | --- |
| `claude-code` | `--mcp-config <run dir>/mcp.json --strict-mcp-config`, plus `mcp__coppice__*` in `--allowedTools` | Not yet |
| `codex` | `-c mcp_servers.coppice.url=…` and `-c mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"` | Not yet |
| `cursor` | Coppice-owned `HOME` with `.cursor/mcp.json`, and `CURSOR_CONFIG_DIR` with a `cli-config.json` allowing `Mcp(coppice:*)` | Yes (`2026.09.28-64d2043`) |
| `opencode` | `OPENCODE_CONFIG=<run dir>/opencode.json` for the per-run `opencode serve` | Yes (`1.18.33`) |
| `kilo-code` | `KILO_CONFIG=<run dir>/kilo-config.json` (name follows OpenCode's convention; may differ in Kilo) | Not yet |

Plugin MCP tools are proxied through the same gateway as `<plugin>__<tool>`, so connectors need no per-plugin wiring. More in [architecture.md](../architecture.md#plugin-mcp-proxy).

## Diagnostics (Tools → Connectors)

Tools → Connectors shows each connector's switch, CLI readiness, detected auth (env var names and file paths only, never values), probe output, last real run, and last test.

**Test connection** runs a real agent run through the production path against a synthetic ticket in a scratch directory. It passes when the agent calls `ticket_get` and `result_submit` and submits `done`. The check runs with read-only tools, so on `kilo-code` it fails with the read-only refusal above.

`coppice connector doctor <id>` runs the same local checks from a terminal.

## Tests

No real agent CLI runs in CI or automated tests.

- `MockProvider` (`server/src/providers/mock.rs`) is a test-only provider. It replays JSON fixtures from `fixtures/agent-responses/` (pick one with `MOCK_AGENT_RESPONSE`). It's compiled only with the `mock-provider` Cargo feature, which `embedded-test-db`, `make server`, CI and the Docker Compose image turn on. Desktop installers and release builds leave it out, and CI runs the `release_build` tests to check that. It isn't something users pick.
- Adapters are tested against stand-in binaries: `fake-cli` (Cursor and Kilo Code) and `fake-opencode`.

| Command | What it runs |
| --- | --- |
| `make test` | Full Rust suite (`cargo nextest run --features embedded-test-db --workspace`; falls back to serial `cargo test`) |
| `make test-unit` | Library tests only |
| `make clippy` | Clippy with and without `mock-provider` |
| `make web-test` | Web app tests |
| `cargo test -p coppice-connectors` | Descriptors, probe, sign-in checks |
| `cargo test --features embedded-test-db -p coppice-server --test integration_cli_adapters` | Cursor and Kilo Code adapters against `fake-cli` |
| `cargo test --features embedded-test-db -p coppice-server --test integration_opencode_run_server` | Per-run `opencode serve` against `fake-opencode` |
| `cargo test --features embedded-test-db -p coppice-server --test integration_connector_diagnostics` | Connectors API and Test connection |

To try a real CLI, run Coppice from source or in Docker, create an agent on that connector, and use **Test connection**.

## Docker (optional, for development)

`make compose-up` builds and starts the full stack (web on `http://localhost:5001`; see [development.md](../development.md)). The Compose image includes `mock-provider` and defaults agents to `mock`, so smoke tests run without a CLI.

To use a real connector inside the container, install it and sign in there. CLIs and their sign-ins live in the `connector_data` volume at `/home/coppice`, not in your host home.

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server coppice connector enable <id>
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server coppice connector install <id>
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server coppice connector setup <id>
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server coppice connector doctor <id>
```

`install` runs the vendor installer for Cursor and OpenCode and prints a hint for the others. `setup` runs the vendor sign-in. Outside Docker, the same `coppice connector list | enable | install | setup | doctor` commands work on your host. `enable` writes to `--config`, otherwise `COPPICE_CONFIG`, otherwise `./config.toml` or `deploy/config/config.toml` if present.

## Adding a connector

Follow [architecture.md § Adding a connector](../architecture.md#adding-a-connector): a descriptor entry, a config struct and example section, an adapter with a `ModelCatalog`, a `FACTORIES` entry, and the per-id arms it lists. Add a guide in this folder and a row in the table above.
