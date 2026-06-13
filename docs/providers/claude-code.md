# Claude Code provider

**ID:** `claude-code`  
**Status:** Deferred (post-M04)  
**Stream backend:** `TmuxStream` (planned)

Anthropic Claude Code CLI integration. Modeled after [Multica's provider matrix](https://www.multica.ai/docs/providers).

## Auth

User runs `claude auth` outside Coppice. Coppice cannot inject subscription credentials.

## Planned capabilities

| Capability | Status |
|------------|--------|
| Live stream | `TmuxStream` — raw pane capture |
| Session resume | Expected to work |
| MCP injection | `--mcp-config` per agent |
| Result parsing | Best-effort JSON from terminal tail |

## Context compaction

Claude Code compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

For OpenCode serve-mode compaction knobs, SSE events, and idle-timeout notes, see [opencode.md § Context compaction](opencode.md#context-compaction).

## Why deferred

Subscription CLIs are harder to control than API-key providers. Needs stable tmux capture and stop/kill before implementation.
