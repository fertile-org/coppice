# Kimi Code

Coppice runs Kimi Code over the Agent Client Protocol: `kimi acp`, one process per run, and sets the session's `mode` option to `yolo` so nothing waits on an approval. Checked on Kimi Code 2.2.0.

## Sign in

Run `kimi login` (or configure a provider in `~/.kimi-code/config.toml`). Without a sign-in, `session/new` answers "Authentication required" and the run fails with "Kimi Code isn't signed in. Run `kimi login`, then try again." Coppice has no free sign-in check, so the connector shows **Installed** until a run or **Test connection** proves it.

## How a run works

- The Coppice gateway is passed in `session/new`; its tools are `mcp__coppice__<tool>`.
- Models are the aliases from the session's `model` option. Resume uses `session/resume`.
- `kimi acp` takes no other flags, so ticket attachments are not passed as extra directories.

## Read-only (chat and planning)

Chat and planning tell Kimi that Coppice handles file writes (`fs/write_text_file`) and terminal commands (`terminal/create`). Kimi then sends those to Coppice instead of running them, whatever its mode, and Coppice refuses with "Coppice: this run only allows reading". The turn goes on. Checked against a scripted model in `yolo` mode: a shell command and a file write were refused, also from a subagent, and reading and `result_submit` worked.

A tool from MCP servers you configured in the CLI yourself is not covered and may still change things.

## Tests

- `cargo test --features embedded-test-db -p coppice-server --lib providers::acp_cli` checks the flags for each policy.
- `cargo test --features embedded-test-db -p coppice-server --test integration_acp` covers the shared ACP client with `fake-opencode acp`.
- `tests/real_acp_clis.rs` runs a read-only chat turn against the installed CLI; it is ignored in CI. `COPPICE_REAL_KIMI=<command>` takes a command for `kimi`, such as a wrapper that sets `KIMI_CODE_HOME` to a test provider.
