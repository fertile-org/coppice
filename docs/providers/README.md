# Connectors: developer guide

This page is for building Coppice from source, running it in Docker, or adding a connector. Using the desktop app? Start with [providers.md](../providers.md).

A connector is how Coppice drives one agent CLI. The board, tickets and worktrees work the same for every connector; each agent picks one.

**Source of truth.** [`connectors/src/lib.rs`](../../connectors/src/lib.rs) holds one descriptor per connector: id, display name, binary, sign-in hints, probe, MCP wiring, console kind and capabilities. The server, the `coppice connector` CLI and the web app (through `GET /api/connectors`) all read it. If this page and the descriptor disagree, the descriptor wins.

## The connectors

| Connector | id | Command | How Coppice runs it | Sign-in check | Guide |
| --- | --- | --- | --- | --- | --- |
| Claude Code | `claude-code` | `claude` | Subprocess in the worktree: `claude -p … --output-format stream-json --verbose --permission-mode bypassPermissions` | `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN` set, or `~/.claude/.credentials.json`; otherwise `claude auth status` | [claude-code.md](claude-code.md) |
| Codex | `codex` | `codex` | Subprocess: `codex exec --json --dangerously-bypass-approvals-and-sandbox -C <worktree>` (`--no-daemon` when the CLI is ≥ 0.156) | `OPENAI_API_KEY` set, or `~/.codex/auth.json`; otherwise `codex login status` | [codex.md](codex.md) |
| Cursor | `cursor` | `cursor-agent` | Subprocess in the worktree: `cursor-agent -p … --trust --force --output-format stream-json --workspace <worktree>` | `~/.config/cursor/auth.json` or `~/.cursor/auth.json`. Neither means not signed in | [cursor.md](cursor.md) |
| OpenCode | `opencode` | `opencode` | 2.x: `opencode acp` per run over the Agent Client Protocol, with the structured console; a model ACP doesn't offer (such as `opencode-go/*`) runs with `opencode run --standalone --format json` instead. 1.x: a fresh `opencode serve --hostname … --port …` per run over HTTP/SSE, with a Live Session | `~/.local/share/opencode/auth.json`; otherwise `opencode auth list` | [opencode.md](opencode.md) |
| Kilo Code | `kilo-code` | `kilo` | Subprocess in the worktree: `kilo run --format json --auto --dir <worktree>` | None. `kilo auth` is interactive, so Coppice never runs it | [kilo-code.md](kilo-code.md) |
| GitHub Copilot | `copilot` | `copilot` | `copilot --acp --no-ask-user --allow-all-tools` per run over the Agent Client Protocol, with the structured console | None free; Installed until Test connection | [copilot.md](copilot.md) |
| Qwen Code | `qwen-code` | `qwen` | `qwen --acp --approval-mode yolo --exclude-tools …` per run over ACP | None free; Installed until Test connection | [qwen-code.md](qwen-code.md) |
| Kimi Code | `kimi` | `kimi` | `kimi acp` per run over ACP, session mode `yolo` | None free; Installed until Test connection | [kimi.md](kimi.md) |
| Grok | `grok` | `grok` | `grok agent --always-approve --no-leader stdio` per run over ACP | None free; Installed until Test connection | [grok.md](grok.md) |

### Detection and status

At startup, the server adds common bin dirs to its `PATH` if they exist and aren't already there: `~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `/opt/homebrew/bin` and `/usr/local/bin`. The desktop app also passes in the `PATH` from your login shell. Probes and runs use this same `PATH`.

For each connector, Coppice resolves the binary (the `command` key if set, otherwise the descriptor binary). It runs the probe (10 s cap), then the sign-in check: env var names, credential files, or the CLI's own status command (3 s cap). The sign-in check never opens a browser, prompts, or calls a model. The result is shown in the agent form and in System → Connectors:

| Readiness | Label | When |
| --- | --- | --- |
| `ready` | Ready | Binary found and sign-in verified |
| `found_not_signed_in` | Found, not signed in | Binary found and a reliable check says no sign-in |
| `not_on_path` | Not on your PATH | Binary not found |
| `found` | *(no label)* | Binary found, sign-in can't be checked |

Kilo Code is always `found` once `kilo` is on `PATH`. Coppice makes no sign-in claim for it. Grok's check is `grok models`, which prints "You are logged in…" or "You are not authenticated." without using the plan.

A connector without an `enabled` key is on when it is `ready` or `found`, and off when it is `not_on_path` or `found_not_signed_in`. `enabled = true` or `false` (the switch in System → Connectors, or saving an agent with that connector) overrides that. `GET /api/connectors` lists every connector with `enabled`, and the agent form puts the ones that are on first.

Probes run at server start, on **Run check**, and when **Test connection** starts. They don't run on every page load.

## Configuration

Each connector has a `[agent.connectors.<id>]` section in `config.toml`:

| Key | Connectors | Default |
| --- | --- | --- |
| `enabled` | all | `false` |
| `command` | `cursor` (`cursor-agent`), `opencode` (`opencode`), `kilo-code` (`kilo`) | Claude Code and Codex always run `claude` / `codex` |
| `model_providers` | all | Optional. Empty or omitted uses the built-in list: `claude-code` → `sonnet, opus, haiku, fable`; `codex` → `openai`; `cursor` → `cursor`. `opencode` and `kilo-code` read providers from `opencode models` / `kilo models` (a few seconds; a CLI failure leaves the list empty). A non-empty value replaces that built-in list. Turning a connector on still fills an empty file entry from the descriptor when the descriptor has one |
| `run_timeout_secs` | all | `1800` |
| `serve_hostname` | `opencode` | `127.0.0.1` (each run's server picks a free port; `serve_port` is ignored) |

Each agent picks a connector and a model on the Agents page. When a connector has more than one provider, those names are headings in the model list. Refresh models loads the list again. There is no default-connector setting. With the `mock-provider` feature, creating an agent that omits a connector uses `mock`. Old `default_connector` / `default_provider` keys and `AGENT_DEFAULT_PROVIDER` are ignored.

Where the config lives:

| How you run Coppice | File |
| --- | --- |
| Desktop app | `config.toml` in the app data dir (`~/Library/Application Support/Coppice/` on macOS, `~/.config/Coppice/` on Linux). Only this file is read |
| From source | Defaults, then `~/.config/coppice/config.toml`, then `./config.toml`, then `COPPICE_CONFIG`, then environment variables (last wins) |
| Docker Compose | `deploy/config/config.toml`, mounted at `/etc/coppice/config.toml` |

The System → Connectors switch and saving an agent both turn a connector on immediately. They write into `config.toml` and keep comments and other keys. `coppice connector enable` and hand edits apply the next time the server starts.

## Chat and session resume

Coppice stores the full transcript itself. When a connector supports resume, later turns reuse the CLI session with a slim context; if resume fails, one retry sends the full transcript.

Chat is on for Claude Code, Codex, Cursor, Kilo Code, and OpenCode. There is no repository picker. Each chat session works in its own folder and can read every linked repository. `REPOS.md` in that folder lists the name, folder, one-line description, and boards for each repository, and marks the current board's repository as the focus. That focus repository is copied when the chat starts. Any other repository is copied when the agent calls `open_repo`. Claude Code, Cursor, and Kilo Code have not been shown to stay read-only, so chat shows "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer." OpenCode and Codex passed a live write test, so they show no banner. Chat still uses the same per-chat copies rather than the registered checkout. OpenCode's read-only rules still apply. MCP tools or a copy's `.opencode/` plugins may still bypass those denials.

A copy is `git clone --local --shared --no-checkout` of the registered checkout, then a checkout of the default branch (or the current commit when that branch is missing), with every remote removed. `--shared` borrows objects through `.git/objects/info/alternates` instead of hardlinks. Each chat gets its own copy. Nothing is shared between chats. Copies are deleted when the chat is archived, after 12 hours without a turn (the opened-repo list is kept and rebuilt on the next visit), and the whole chat folder is deleted after 30 days without a turn. The session row stays. At most 5 copies stay checked out; opening another closes the least recently used one. The board repository is not copied at start when its files exceed 64 MiB or the copy takes longer than 3 seconds; it stays listed for `open_repo`, which may run for 45 seconds. These are `[agent]` settings: `chat_open_repo_cap`, `chat_eager_copy_timeout_secs`, `chat_copy_timeout_secs`, `chat_eager_copy_max_bytes`, `chat_copy_idle_secs`, and `chat_folder_max_age_secs`. The idle and age sweeps run at startup and then hourly. The board's repository is the one its tickets use most.

| Connector | Chat session resume | Where chat runs |
| --- | --- | --- |
| `claude-code` | `--resume` | On, in the per-chat folder, with the untested banner. Flags are `--permission-mode dontAsk` (Claude Code 2.1.285 and later; older versions get no mode flag), `--settings {"disableAllHooks":true}`, `--setting-sources user`, plus `--disallowedTools Write,Edit,MultiEdit,NotebookEdit,Bash,TodoWrite,Task` and a read-only `--allowedTools` list ending in `mcp__coppice__*`. `--strict-mcp-config` loads only the Coppice server. `--settings` is above user, project, and local files, so `disableAllHooks` turns those hooks off. Managed hooks still run. Not live-verified: the `claude` binary was not installed and no API key was set |
| `cursor` | `--resume` | On, in the per-chat folder, with the untested banner. Chat passes `--mode ask` and omits `--force`. Plan runs pass `--mode plan`. `--mode ask` was not shown to block shell. CLI `2026.10.14-941492e` was installed and not signed in; a prompt exited with `Authentication required`. Cursor staff say ask mode does not hard-block shell unless `--sandbox enabled`, which also blocks network and MCP. Ticket runs still pass `--force` |
| `codex` | Skipped | On, in the per-chat folder, no banner. Chat passes `--sandbox read-only`, `--ignore-user-config`, and `-c` keys for `mcp_servers.coppice` only. On `codex-cli 0.162.1`, `--ignore-user-config` skips user and project MCP servers (project trust lives in the user config) and auth still uses `CODEX_HOME`. On 0.163.0 (Linux, signed in) `--sandbox read-only` blocked a shell write and a file write (`tests/real_codex.rs`). Every run passes `--skip-git-repo-check`, because chat folders are not git repositories. Ticket runs still use the bypass flag and still load user config |
| `opencode` | Prior session id | On, in the per-chat folder, no banner. Per-run config sets `permission.edit`, `permission.write`, and `permission.bash` to `deny`, and the process sets `OPENCODE_DISABLE_PROJECT_CONFIG=1`. Verified with OpenCode 1.19.0 and `opencode/big-pickle`: denying edit and bash blocked a file write and a shell write even when the project `opencode.json` allowed both. The same deny without the env var lost to the project file. MCP tools or `.opencode/` plugins may bypass those denials |
| `kilo-code` | `--session` | On, in the per-chat folder, with the untested banner. Chat passes `--auto --agent ask`. Plan runs pass `--auto --agent plan`. Later chat turns pass `--session <id>` from the previous turn's JSON `sessionID`. If that run exits non-zero, emits an error event, or returns no output, the stored id is cleared and the turn is retried once without `--session`, with the same bounded transcript. Ask mode denies writes; `--auto` still approves permissions that are not denied, including MCP. A live write was not shown to be blocked. Not tried against a real `kilo` binary. Test connection still runs `kilo --version`. Ticket runs still use `kilo run --auto` and do not resume |
| `copilot` | `session/load` | On, in the per-chat folder, no banner. Chat and planning add `--deny-tool shell --deny-tool write --disable-builtin-mcps`; deny beats `--allow-all-tools`. Checked on 1.0.98 with a real model: a shell command and a file write did not run, reading and `result_submit` did |
| `qwen-code` | `session/resume` | On, in the per-chat folder, no banner. Chat and planning also exclude `run_shell_command`, `write_file`, `edit`, and `notebook_edit`; the `tool_call` bridge cannot reach an excluded tool. Plan mode is never used: on 0.25.2 it refused the gateway's MCP call. Checked against a scripted model |
| `kimi` | `session/resume` | On, in the per-chat folder, no banner. Chat and planning offer to handle file writes and terminal commands, Kimi routes them to Coppice, and Coppice refuses; a subagent's writes go the same way. Checked on 2.2.0 against a scripted model |
| `grok` | `session/resume` | On, in the per-chat folder, no banner. Chat and planning set `GROK_SANDBOX=read-only`, which blocks file edits outside temp folders, and offer to handle terminal commands, which Grok routes to Coppice and Coppice refuses. Grok's edit tools are not routed, so a read-only run in a temp folder is refused. Checked on 1.0.62 with the real model |

Ticket runs can resume a prior session on `claude-code` and `cursor`. The other connectors start a fresh session and continue from the ticket checkpoint.

## Coppice MCP gateway (tool-first runs)

Every run gets a per-run token for the Coppice MCP gateway at `POST /mcp`, handed to the CLI as `COPPICE_MCP_URL` / `COPPICE_MCP_TOKEN`. Each connector wires the gateway for that run only, through flags, process env, or a file under `<artifacts_dir>/runs/<run id>/`. Cursor is the exception: the CLI has no MCP flag, so Coppice writes `<worktree>/.cursor/mcp.json` for the run and restores it after. The token is never written to a file. If a connector can't be wired, the run fails with `mcp_unavailable`.

| Connector | Mechanism | Checked against a live CLI |
| --- | --- | --- |
| `claude-code` | `--mcp-config <run dir>/mcp.json --strict-mcp-config`, plus `mcp__coppice__*` in `--allowedTools` | Not yet |
| `codex` | `-c mcp_servers.coppice.url=…` and `-c mcp_servers.coppice.bearer_token_env_var="COPPICE_MCP_TOKEN"` | Not yet |
| `cursor` | `<worktree>/.cursor/mcp.json` (restored after the run) and `CURSOR_CONFIG_DIR` with a `cli-config.json` allowing `Mcp(coppice:*)`. Real `HOME` is kept so Keychain login still works | Yes (`2026.10.22-27dcf5f`) |
| `opencode` | `OPENCODE_CONFIG=<run dir>/opencode.json` for the per-run `opencode serve` | Yes (`1.18.33`) |
| `kilo-code` | `KILO_CONFIG=<run dir>/kilo-config.json` (name follows OpenCode's convention; may differ in Kilo) | Not yet |

Plugin MCP tools are proxied through the same gateway as `<plugin>__<tool>`, so connectors need no per-plugin wiring. More in [architecture.md](../architecture.md#plugin-mcp-proxy).

## Diagnostics (System → Connectors)

System → Connectors shows each connector's switch, CLI readiness, detected auth (env var names and file paths only, never values), probe output, last real run, and last test.

**Test connection** runs a real agent run through the production path against a synthetic ticket in a scratch directory, once for each mode Coppice launches the connector in: **Chat** (read-only chat flags), **Planning** (the planning flags), then **Ticket work** (the write flags). Each phase must call `ticket_get` and `result_submit` and submit `done`. In a phase that must block writes (chat, and planning where the connector enforces read-only), the agent is also asked to create `coppice-write-check.txt`; if that file appears, the check fails with "this mode did not block writes". The check stops at the first failing phase and names it, for example `Chat: result_submit was not called`. Every connector, Kilo Code included, gets this real check. It sends three prompts, so it only runs when you click it.

`coppice connector doctor <id>` runs the same local checks from a terminal.

## When a run is stopped or fails

- **Timeouts.** Besides each connector's `run_timeout_secs`, a stream-JSON CLI (Claude Code, Codex, Cursor, Kilo Code) is stopped when it prints nothing in its first 3 minutes, or when it prints nothing for 20 minutes while no tool is running. A running tool, such as a long build, may be quiet for longer. OpenCode 2 over ACP has its own limits (see [opencode.md](opencode.md)).
- **Errors.** Every exit, timeout, stall, and error result carries the CLI's stderr. Claude Code's `is_error` and `terminal_reason` (`prompt_too_long` fails even without the error flag) and Codex's `turn.failed` fail the run with the CLI's own message. A failed or refused `result_submit` is named with Claude Code's permission mode.
- **Chat replies.** A chat turn that ended cleanly with plain text but no `result_submit` posts that text as the reply. A turn whose `result_submit` failed is a setup problem and fails instead.
- **Resume.** A failure counts as "session gone" only when it says so (`providers/resume.rs`); then chat retries once in a fresh session. Network, quota, and sign-in failures keep the session.
- **Version floors.** Claude Code needs 2.0.0 and OpenCode 1.1.54. An older CLI fails at run start with both versions.
- **Each run** logs one `cli stream summary` line: lines read, JSON lines, whether a terminal event arrived, how it ended, the exit code, and the duration.

## Tests

No real agent CLI runs in CI or automated tests. Output captured from a real CLI is kept as `fixtures/<connector>/<cli>-<version>-<case>.jsonl` (for example `claude-code-2.1.302-plan-submit-refused.jsonl`, `opencode-2.0.20-read-only-turn.jsonl`), with local paths and account details removed, and replayed through the connector's parser in its tests. When a CLI release changes its output, add a file for the new version next to the old one instead of editing it. [CLI integration review](cli-integration-review.md) lists the gaps this leaves, how to close them, and planned connector changes.

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
| `cargo test --features embedded-test-db -p coppice-server --test integration_acp` | The ACP client against `fake-opencode acp` scenarios: gateway, model, read-only refusal, resume, idle, cancel, oversized lines, early exit. `COPPICE_REAL_OPENCODE=1 … real_opencode -- --ignored` runs a read-only turn on the installed OpenCode 2 |
| `cargo test --features embedded-test-db -p coppice-server --test integration_connector_diagnostics` | Connectors API and Test connection |

To try a real CLI, run Coppice from source or in Docker, create an agent on that connector, and use **Test connection**.

## Docker (optional, for development)

`make compose-up` builds and starts the full stack (web on `http://localhost:5001`; see [development.md](../development.md)). The Compose image includes `mock-provider`, so creating an agent without a connector uses `mock` and smoke tests run without a CLI.

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
