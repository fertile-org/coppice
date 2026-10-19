# Qwen Code

Coppice runs Qwen Code over the Agent Client Protocol: `qwen --acp --approval-mode yolo`, one process per run. Checked on Qwen Code 0.25.2.

## Sign in

Run `qwen` in a terminal and sign in, or set `OPENAI_API_KEY` for an OpenAI-compatible provider. Coppice has no free sign-in check, so the connector shows **Installed** until a run or **Test connection** proves it.

## How a run works

- Every run excludes `ask_user_question`, `enter_plan_mode`, and `exit_plan_mode`: nobody can answer a question, and plan mode refuses the gateway's MCP calls on 0.25.2.
- The Coppice gateway is passed in `session/new`. Qwen loads MCP tools lazily: the agent finds `mcp__coppice__<tool>` with `tool_search` and calls it through `tool_call`.
- Models come from the session (ids look like `name(authType)`). Resume uses `session/resume`.
- Ticket attachments are passed with `--add-dir`.

## Read-only (chat and planning)

Chat and planning also exclude `run_shell_command`, `write_file`, `edit`, and `notebook_edit`, so the model never sees them, and the `tool_call` bridge refuses them by name ("not registered in this session"). Checked against a scripted model: no file was written and reading worked.

A tool from MCP servers you configured in the CLI yourself is not covered and may still change things.

## Tests

- `cargo test --features embedded-test-db -p coppice-server --lib providers::acp_cli` checks the flags for each policy.
- `cargo test --features embedded-test-db -p coppice-server --test integration_acp` covers the shared ACP client with `fake-opencode acp`.
- `tests/real_acp_clis.rs` runs a read-only chat turn against the installed CLI; it is ignored in CI. `COPPICE_REAL_QWEN=<command>` takes a command for `qwen`, such as a wrapper that points it at a test model.
