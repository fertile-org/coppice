# Claude Code provider

**ID:** `claude-code`  
**Status:** Implemented  
**Stream backend:** subprocess stdout (stream-json)

Anthropic Claude Code CLI integration via subprocess. Agents run as `claude -p` processes with subscription (OAuth token) auth.

## Auth

The host operator generates an OAuth token with `claude setup-token` and stores it in an environment variable (default: `CLAUDE_CODE_OAUTH_TOKEN`). Coppice reads this variable from the server's environment and injects it into the child process. `ANTHROPIC_API_KEY` is explicitly unset to ensure subscription auth only.

```toml
[agent.connectors.claude-code]
enabled = true
# command = "claude"
# run_timeout_secs = 600
# oauth_token_secret = "CLAUDE_CODE_OAUTH_TOKEN"
# model_providers = ["sonnet", "opus", "haiku"]
```

## Capabilities

| Capability | Status |
|------------|--------|
| Subprocess execution | `claude -p --output-format stream-json --verbose` |
| Session ID capture | Extracted from stream-json events, persisted to run |
| Model selection | `--model` from agent config |
| Result parsing | `extract_result_from_text` on final assistant / result text |
| Cancellation | `cancel_rx` kills the subprocess |
| Timeout | Configurable via `run_timeout_secs` (default 600s) |
| Live stream | Forwarded to run stream as frames (basic) |
| Session resume | Follow-up ticket |
| MCP injection | Follow-up ticket |

## How it works

1. Coppice spawns `claude -p "<coppice_run_prompt>" --output-format stream-json --verbose --allowedTools ... --permission-mode bypassPermissions` with CWD set to the agent worktree.
2. Each stdout line is a JSON event. The provider accumulates assistant text deltas and captures `session_id` from the first event that contains it.
3. The terminal `result` event provides the final assistant text. Coppice extracts the JSON contract (`AgentRunResult`) from that text using `extract_result_from_text`.
4. On cancel or timeout, the subprocess is killed.

## Context compaction

Claude Code compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).
