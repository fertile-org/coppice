# GitHub Copilot CLI

Coppice runs GitHub Copilot CLI over the Agent Client Protocol: `copilot --acp --no-ask-user`, one process per run, in the ticket worktree or the chat folder. Checked on Copilot CLI 1.0.98.

## Sign in

Run `copilot login` (or set `COPILOT_GITHUB_TOKEN`). Coppice has no free sign-in check, so the connector shows **Installed** until a run or **Test connection** proves it.

## How a run works

- Ticket runs add `--allow-all-tools`, so nothing waits on a prompt.
- The Coppice gateway is passed in `session/new` as an HTTP MCP server with a `Bearer` header; its tools show up as `coppice-<tool>`. Copilot first calls a non-standard `server/discover` method, which the gateway answers with "method not found", and then carries on.
- Models come from the session's `model` option. Resume uses `session/load`.
- Ticket attachments are passed with `--add-dir`.

## Read-only (chat and planning)

Chat and planning add `--deny-tool shell --deny-tool write --disable-builtin-mcps`. Copilot's permission docs say deny rules win over `--allow-all-tools`; the built-in GitHub MCP server is turned off because it can act on GitHub with your account. Checked with a real model: a shell command and a file write were denied, the turn went on, reading worked, and `result_submit` reached the gateway.

A tool from MCP servers you configured in the CLI yourself is not covered and may still change things.

## Tests

- `cargo test --features embedded-test-db -p coppice-server --lib providers::acp_cli` checks the flags for each policy.
- `cargo test --features embedded-test-db -p coppice-server --test integration_acp` covers the shared ACP client with `fake-opencode acp`.
- `tests/real_acp_clis.rs` runs a read-only chat turn against the installed CLI; it is ignored in CI. `COPPICE_REAL_COPILOT=1` uses the signed-in `copilot`.
