# CLI integration review

Checked 2026-10-18 on Linux against Claude Code 2.1.302, Cursor Agent 2026.10.01, OpenCode 2.0.20, and Kilo Code 7.8.3. Codex was not installed, so its section is from the CLI's documented protocol and is not verified here.

Coppice drives five agent CLIs. Each one changes its flags, modes, and output between releases, and Coppice finds out when a user's run breaks. The rc.8 chat bug is the example: Claude Code 2.1.290 started refusing MCP tools that are not read-only in plan mode, so `result_submit` failed, the streamed reply vanished, and nothing told the user why.

This review looks at how Coppice launches each CLI, reads its output, decides the outcome, enforces read-only, resumes, and tests all of that. It lists what to keep, what to change, and in what order.

**Scope.** Changes stay inside `connectors/` and `server/src/providers/` (plus `sessions/` for OpenCode). User flows don't change: Plan Review, Human Review, chat, tickets, the result contract, and the MCP gateway all stay as they are.

## Keep: what Coppice already does well

- **The MCP gateway and `result_submit`.** A structured result submitted through a tool call beats scraping text from a stream. It carries plan steps, acceptance criteria, splits, questions, and handoffs, and it is validated before it is stored (`mcp/tools/result.rs`). Keep it as the primary path. The changes below add fallbacks around it, not a replacement.
- **The run contract descriptor** (`connectors/src/run_contract.rs`). Pinned flags, per-flag minimum versions, and the reason for each flag in one place shared by the server and the CLI.
- **Per-run config isolation.** The token stays in the environment and is interpolated into a per-run config (`{env:COPPICE_MCP_TOKEN}`), and no global CLI config is touched.
- **OpenCode read-only by policy.** OpenCode chat denies `edit`, `bash`, and `write` in its per-run config, and `OPENCODE_DISABLE_PROJECT_CONFIG` stops a project file from allowing them back. That is enforcement Coppice owns, not a CLI mode whose meaning can change. It is the model the other connectors should move towards.
- **Test connection** (`workers/job_worker/connector_check.rs`). A real run through the production path, which passes only when the agent calls `ticket_get` and `result_submit`. It would have caught the plan-mode bug on Claude Code 2.1.302. It just doesn't cover every mode Coppice uses (see [Testing](#testing)).
- **Submitted result before parsed text** (`prefer_submitted_result`). A run that submitted a result keeps it even if the stream ends badly.

## Urgent findings

### OpenCode 2.x: the server API moved and now needs a password

Verified with OpenCode 2.0.20, without sending a prompt:

- `opencode serve` now serves the web app at `/`. `GET /session` returns HTML and `POST /session` returns 405. The API is under `/api/` (`/api/session`, `/api/config`, …).
- Every `/api/` call needs HTTP basic auth. The server prints a generated password at startup. `OPENCODE_SERVER_PASSWORD` sets it, and the user name is `opencode`.
- `OPENCODE_CONFIG` still reaches `opencode serve`. `/api/config` lists the file, and the server converts the 1.x shape (`permission`, `mcp`) to its 2.x shape (`permissions` rules, `mcp.servers`) and resolves `{env:…}`.

Coppice's client (`sessions/opencode_client.rs`) calls `/session`, `/session/{id}/prompt_async`, `/session/{id}/abort`, `/session/{id}/message`, `/session/status`, and `/event`, with no auth. On OpenCode 2.x it gets HTML or 405, so the OpenCode connector most likely fails on every run. Users installing OpenCode today get 2.x.

**Change:** detect the major version (already probed by `cli_version`), and for 2.x set a random per-run `OPENCODE_SERVER_PASSWORD`, send basic auth, and use the `/api/` paths. Then re-check the session, prompt, message, and event shapes against 2.x and record them as fixtures. Keep the 1.x path until 1.x is out of support.

### Cursor and Kilo Code use CLI modes for read-only

Claude Code's plan mode changed meaning in a minor release. Cursor chat uses `--mode ask` and planning uses `--mode plan`; Kilo Code uses `--agent ask` and `--agent plan`. Any of these can start refusing MCP calls the same way, and the result is the same lost reply. Kilo Code already hit a version of this, which is why it keeps `--auto` next to `--agent ask`.

**Change:** cover chat and plan modes in Test connection (below) so a CLI update shows up as a failed check, not a user's lost reply.

## Shared changes (all connectors)

### 1. One terminal-outcome contract

Today each provider decides the outcome its own way. Cursor reads `is_error` and attaches stderr to failures. Kilo Code attaches stderr. Claude Code and Codex do neither: a failed Claude run ends as "no result contract found in claude-code output", which hides the real cause.

Put the rules in one place (`cli_runner` or a small `terminal.rs`) and apply them to every stream-JSON connector, in this order:

1. A structured failure field on the terminal event wins. Claude Code's result event has `terminal_reason`. `prompt_too_long` has been seen arriving with `is_error: false`, so check it on its own.
2. `is_error: true` fails the run with the event's text, or "returned an error without details".
3. Cancelled → cancelled. Deadline → timed out.
4. A stdout read error fails the run.
5. A non-zero exit fails the run unless a terminal result was already read (`had_terminal_result`, as now).
6. No terminal event at all fails the run: "stream ended without a result".
7. Every failure message gets the stderr tail. `cli_runner` already keeps it, but most providers drop it.

A failed run returns no output, so a partial transcript is never mistaken for a final answer.

### 2. A fallback chain around `result_submit`

Keep `result_submit` as the primary path. When it never arrives:

1. **Ask the CLI why.** Claude Code's result event lists `permission_denials`. If `mcp__coppice__result_submit` is in it, fail with "Claude Code refused result_submit" and the permission mode from the init event. That turns the rc.8 bug into a one-line diagnosis.
2. **Parse a result from the final text** (`extract_result_from_text`, as now).
3. **Chat only:** if the turn succeeded by rules 1–6 above and the final text is non-empty, post it as the reply and log that it was not submitted. A chat reply needs no structure, so losing it to a missing tool call helps nobody. Ticket, plan, and verify runs keep failing, because they need the structured fields.

The failed-turn system note in chat (`note_failed_chat_turn`) stays for the cases this can't save.

### 3. Fail fast when the MCP gateway didn't connect

Claude Code's first event (`type: system`, `subtype: init`) lists `mcp_servers` with a `status` per server, plus `permissionMode` and the `tools` the session has. If the `coppice` server isn't `connected`, end the run right away with `mcp_unavailable` instead of letting the agent work for minutes and then fail with "no result". The `tools` list also shows, for free, whether `mcp__coppice__result_submit` is available and whether `Write`, `Edit`, or `Bash` leaked into a read-only run. Codex and OpenCode report MCP server state too, so do the same there once their events are mapped.

### 4. Liveness timeouts, not only a wall clock

Each connector has one hard `run_timeout_secs`. A long run that keeps working gets killed, and a stuck run waits the full timeout. Add:

- **Startup:** no first event within N seconds of launch (CLI hung on auth, a prompt, or a bad flag).
- **No progress:** the turn started but nothing meaningful arrived (no text, tool call, or result) for N minutes. Count tool calls in flight as progress so a long build is not killed.
- **Handshake:** for server-style connectors (OpenCode serve, and Codex if it moves to `app-server`), a bound on each startup request.

Keep the wall clock as the outer safety bound. Each timeout names itself in the error ("no output for 10 minutes"), so the person knows what happened.

### 5. Resume: decide precisely when to start fresh

Retrying a resume with a fresh session throws away the agent's memory, so it should only happen when the session is really unusable:

- **Start fresh** when the CLI says the session is gone or belongs to another account. Keep the exact phrases as a list in one place, each with the CLI version it was seen on. For example, Claude Code prints "No conversation found" when `--resume` names a missing session.
- **Start fresh** when the session's context is exhausted (`terminal_reason: prompt_too_long`), and don't resume that session again.
- **Don't start fresh** on network errors, rate limits, quota, provider 5xx, or auth errors. Those keep the session for the next try.

Chat already retries once with the transcript (`is_resume_session_invalid`). Give it the shared phrase list instead of per-connector checks.

### 6. Process lifecycle

- Close stdin once the terminal result is read, so a CLI waiting on input exits. When Coppice keeps stdin open for a control protocol (Claude Code hooks, Codex app-server), it must keep reading stdout while it writes, or the CLI blocks on a full stdout pipe before it reads its first input line.
- On cancel or timeout, send SIGTERM to the whole process group, wait a short grace period until the group is empty, then SIGKILL the group. Waiting on the leader alone lets a child that ignores SIGTERM outlive the run. Check that `agent-processes.json` tracking does this for every connector.
- Claude Code can run Bash in the background (`run_in_background`) and end its turn before that work finishes. Coppice already ends the run at the result (`result_before_exit`). Also record when a background launch happened, so the run log says why something kept running.

### 7. Bounded stdout lines

`cli_runner` reads stdout with `BufReader::lines()`, which has no length limit. One event can carry a whole conversation (a resumed thread, a large tool result), and a misbehaving CLI can emit one endless line. Cap a line at 32 MiB, and fail with a clear "event too large" error instead of growing without bound. OpenCode's SSE buffer already has a 16 MiB cap (#161).

### 8. Environment hygiene

When Coppice starts from a terminal that is itself inside Claude Code, the child inherits markers that make it think it is a nested or resumed session. Remove exactly these before launch: `CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_EXECPATH`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_SSE_PORT`, and anything starting with `CLAUDECODE_`. Keep every other `CLAUDE_CODE_*` variable. Those are user settings (`CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_GIT_BASH_PATH`, …), and removing the whole prefix breaks them.

### 9. Minimum versions

`cli_version` detects the version and gates single flags. Add a floor per connector, below which a run is refused with "Coppice needs Claude Code 2.1.285 or later". That beats a confusing failure halfway through a run. Show the floor on the Connectors page.

### 10. One protocol summary line per run

At the end of every run, log one line: CLI, version, model, exit code, events read, unreadable lines, assistant and tool events, whether a terminal event arrived and whether it was an error, result bytes, and the stdout read error if any. When a user reports a broken run, that line tells which part of the protocol failed without replaying the transcript.

## Per connector

### Claude Code

| Area | Now | Change |
|------|-----|--------|
| Read-only | `--permission-mode dontAsk` from 2.1.285, plus allow and deny lists (a0cf4b4) | Keep. Later, enforce by policy (below). |
| Outcome | Final text, then `extract_result_from_text` | Shared contract: `terminal_reason`, `is_error`, stderr tail |
| No result | "no result contract found" | Name `permission_denials`; chat text fallback |
| MCP | Wired per run with `--strict-mcp-config` | Fail fast on init `mcp_servers` status |
| Resume | `--resume <id>` | Shared rejection phrases; retire on `prompt_too_long` |

**Read-only by policy (later).** With `--input-format stream-json`, Claude Code accepts control frames on stdin. Coppice can send an `initialize` control request that registers a `PreToolUse` hook, and Claude Code calls back (`hook_callback`) before each tool runs. Coppice answers allow or deny from its own rule (allow `Read`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `mcp__coppice__*`; deny the rest). Read-only would then hold no matter what a permission mode means in the next release, the way OpenCode's config rules do. That needs stdin kept open and stdout read in parallel (shared change 6), and the prompt sent as a `{"type":"user",…}` frame instead of after `-p`. Keep `dontAsk` and the tool lists as a second layer.

### Codex

| Area | Now | Change |
|------|-----|--------|
| Invocation | `codex exec --json`, prompt on stdin | Keep for now; consider `app-server` (below) |
| Read-only | `--sandbox read-only` for chat; plan runs use the write flags in a scratch copy | Keep; add the plan mode to Test connection |
| Outcome | Agent messages, `turn.completed` is terminal | Shared contract; read `turn.failed` and error items; stderr tail |
| MCP isolation | `-c mcp_servers.…` overrides plus `--ignore-user-config` for chat | Keep. `-c mcp_servers={}` doesn't clear the table (noted in `codex.rs`), so keep the effective-servers check |

**`codex app-server` (bigger change).** `codex app-server --listen stdio://` speaks JSON-RPC: `initialize`, `thread/start` or `thread/resume`, `turn/start`, then `item/*` and `turn/*` notifications until `turn/completed`. Codex asks the host to approve each step (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `item/permissions/requestApproval`, `mcpServer/elicitation/request`). For Coppice that means:

- Read-only chat and planning by policy: decline file changes and commands, accept the Coppice MCP tools. Codex chat could then move from the untested banner to verified.
- A real interrupt (`turn/interrupt`) instead of killing the process.
- A clean resume: try `thread/resume`, fall back to `thread/start` when it is refused, but not on a transport error.
- Its own failure modes to handle: a turn that starts and never emits anything (needs the no-progress timeout), a model catalog that fails to load at startup (retry with backoff), and a resumed thread that comes back as one very large line (needs the line cap).
- Any request method Coppice doesn't know is answered with an error and fails the turn with that method's name, so a new approval type is visible instead of hanging.

`app-server` needs Codex 0.100.0 or later. Do it after the shared changes, behind the version check, and keep `exec` for older installs.

### Cursor

| Area | Now | Change |
|------|-----|--------|
| Read-only | `--mode ask` (chat), `--mode plan` (planning) | Cover both in Test connection; a mode change is the main risk |
| Outcome | `is_error` read, stderr tail attached | Already closest to the shared contract; move it there |
| MCP | Per-run `HOME` for its config | Keep; fail fast if Cursor reports the server failed |

Cursor can run shell commands in the background that outlive its result. Track them and stop them when the run ends, within one overall time budget for the cleanup, not one per process.

### OpenCode

| Area | Now | Change |
|------|-----|--------|
| Server | One `opencode serve` per run, 1.x API paths, no auth | **2.x adapter: `/api/` paths, per-run `OPENCODE_SERVER_PASSWORD`, basic auth** |
| Read-only | `permission` deny rules in `OPENCODE_CONFIG`, project config disabled | Keep; verify the 2.x converted `permissions` still deny `edit` and `shell` |
| Config | `OPENCODE_CONFIG` file, token via `{env:…}` | Keep; still honoured by `serve` in 2.0.20 |
| Liveness | SSE with a 16 MiB buffer cap | Add the startup and no-progress timeouts |

Also on 2.x: the model variant moved into the model string as `provider/model#variant`. Check that the model picker's values still work.

### Kilo Code

| Area | Now | Change |
|------|-----|--------|
| Invocation | `kilo run --format json --auto --dir <worktree>` | Watch the OpenCode 2.x changes; Kilo Code follows OpenCode |
| Read-only | `--agent ask` with `--auto` | Cover chat and plan in Test connection |
| Test connection | `kilo --version` only | Real check run, like the other connectors |

OpenCode 2.0 removed `--dir` and `--variant` from `opencode run` (2.0.20's `run --help` lists neither; Kilo Code 7.8.3 still has `--dir`), and they fail with "Unrecognized flag". `opencode run` also became a thin client for a background service, so killing its process group no longer stops the work. When Kilo Code takes those changes, `--dir` will break the same way. Pin the working directory through the process cwd (already set) so `--dir` can be dropped, and gate `--dir` on the versions known to accept it.

## Testing

The bugs above share one cause: tests check the flags Coppice builds, not what the CLI does with them.

### 1. Recorded transcripts per CLI version

Replace the hand-written fixtures in `fixtures/<connector>/` over time with captured output, named `<cli>-<version>-<case>.jsonl`, for example `claude-code-2.1.302-dontask-done.jsonl`. Capture at least these cases per connector: done, blocked, error result, refused tool (`permission_denials`), context exhausted, resume rejected, MCP server failed, and cancelled. When a CLI release changes its output, add a new file next to the old one instead of editing it, so both versions stay covered.

### 2. Test connection covers every mode Coppice uses

Today Test connection runs one read-only run. Run one short check per mode the connector is used in: ticket (write), chat (read-only), and plan. In each read-only mode, the check:

- calls `ticket_get` and `result_submit`, as now
- asks the agent to create a marker file in its scratch dir, then confirms the file doesn't exist

A connector whose read-only checks pass on the installed version can drop the "Coppice hasn't confirmed …" chat banner for that version, and one that fails says which mode failed. Kilo Code gets a real check instead of `kilo --version`. These checks send a prompt, so they stay behind **Test connection** and never run as a background readiness probe.

### 3. A developer check without a Coppice server

`scripts/cli-conformance` (new) runs each installed CLI with the exact argv Coppice builds, against a small stand-in MCP server that records tool calls. It reports for each mode: whether writes and Bash were blocked, whether `result_submit` arrived, and the terminal event fields. That is how the rc.8 fix was verified by hand: `dontAsk` blocked `touch` and file writes and let `result_submit` through on 2.1.302, and so did default mode with the same tool lists. Run it before each release and when a CLI ships a new version.

## Order of work

| Priority | Change | Why |
|----------|--------|-----|
| P0 | OpenCode 2.x adapter | Current OpenCode installs most likely can't run |
| P0 | Test connection per mode, Kilo Code real check | Catches the next mode change before users do |
| P1 | Shared terminal contract, stderr tail, line cap | Clear errors instead of "no result contract found" |
| P1 | `permission_denials` diagnosis, chat text fallback | Saves chat replies and names the cause |
| P1 | MCP init status fail-fast | Minutes saved on every broken gateway |
| P1 | Environment hygiene, minimum versions | Small, removes whole classes of confusing failures |
| P2 | Liveness timeouts | Long healthy runs survive, stuck runs end early |
| P2 | Recorded transcripts, conformance script | Tests that follow the CLIs |
| P2 | Shared resume rejection list | One place to update per CLI release |
| P3 | Claude Code read-only by hook policy | Read-only that no CLI mode change can break |
| P3 | Codex `app-server` | Read-only by approval, real interrupt, cleaner resume |

Adding human messages to a run that is still going (Claude Code hooks and Codex `turn/steer` both allow it) is a new feature and out of scope here.
