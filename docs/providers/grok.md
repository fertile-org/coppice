# Grok

Coppice runs Grok over the Agent Client Protocol: `grok agent --always-approve --no-leader stdio`, one process per run. Checked on Grok 1.0.62 with the signed-in `grok-4.7` model.

## Sign in

Run `grok login` (or set `XAI_API_KEY`). Grok keeps its sign-in in `~/.grok/auth.json`. Coppice checks the sign-in with `grok models`, which lists models without using your plan, so the connector shows **Ready** or **Found, not signed in**. When Grok answers a prompt with `Rate limited`, the run fails with Grok's own message, which names the limit.

## How a run works

- The Coppice gateway is passed in `session/new`. Grok loads MCP tools lazily (`search_tool`, then `use_tool`), and the gateway's tools show as `coppice__<tool>`.
- Models are the values of the session's `model` option. Resume uses `session/resume`.
- Live Console results: built-in tools come from ACP `content`. Coppice MCP and Bash results come from `rawOutput` (`output` MCP envelope, `output_for_prompt`), because Grok sends no text `content` for them.
- `--always-approve`: refusing a Grok permission prompt cancels the whole turn, so no run waits on one.
- `--no-leader`: each run is its own process. Grok refuses a sandbox in leader mode, and a shared leader would outlive the run.
- `grok agent` takes no `--add-dir`, so ticket attachments are not passed as extra directories. The read-only sandbox can still read them.

## Read-only (chat and planning)

Two layers, because Grok handles shell commands and file edits differently:

- **Shell commands:** chat and planning tell Grok that Coppice handles terminal commands (`terminal/create`). Grok sends them to Coppice, which refuses with "Coppice: this run only allows reading". The turn goes on.
- **File edits:** Grok's edit tools write directly and are not routed to the client. Read-only runs set `GROK_SANDBOX=read-only`, a kernel sandbox (Landlock or bubblewrap on Linux, Seatbelt on macOS) that lets Grok read everywhere and write only to `~/.grok` and temp folders. An edit fails and the turn goes on.

Because that sandbox lets Grok write to temp folders, Coppice refuses a read-only Grok run whose folder is inside `/tmp`, `/var/tmp`, or the OS temp folder, with "Grok can't be kept read-only in a temp folder". Coppice's worktrees folder is outside them by default.

Checked with the real model: a shell command and a file write were both blocked, while reading and `result_submit` worked. A tool from MCP servers you configured in the CLI yourself is not covered and may still change things.

## Tests

- `cargo test --features embedded-test-db -p coppice-server --lib providers::acp_cli` checks the arguments and the sandbox variable for each policy.
- `tests/real_acp_clis.rs` runs a read-only chat turn against the installed CLI; it is ignored in CI. `COPPICE_REAL_GROK=1` uses the signed-in `grok`. Its work folder is under `target/tmp` because of the temp-folder rule. The same file checks, without the CLI, that a read-only Grok run in a temp folder is refused.
