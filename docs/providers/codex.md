# Codex provider

**ID:** `codex`  
**Status:** Deferred (post-M04)  
**Stream backend:** `TmuxStream` (planned)

OpenAI Codex CLI integration.

## Auth

User subscription or OAuth via Codex CLI. No API key field in Coppice config.

## Planned capabilities

| Capability | Status |
|------------|--------|
| Live stream | `TmuxStream` |
| Session resume | Unreliable (Multica: "code exists but unreachable") |
| MCP injection | Per-task `$CODEX_HOME/config.toml` |
| Result parsing | Best-effort JSON from terminal tail |

## Context compaction

Codex compacts long session history **within a single run** when context nears the model limit. Coppice relies on this provider guard; it does **not** implement a parallel summarizer or call provider compaction APIs proactively.

For **cross-run continuity**, prefer checkpoint runs: the agent returns `status: "continued"` with a `progressNote`, the human starts the next run, and Coppice injects resume context into `.agent/context.md` (see [Context & Long-Running Tasks design](../superpowers/specs/2026-06-10-context-long-running-tasks-design.md)).

For OpenCode serve-mode compaction knobs, SSE events, and idle-timeout notes, see [opencode.md § Context compaction](opencode.md#context-compaction).

## Why deferred

Harder to control than attach/API providers. Document first; implement when `TmuxStream` and stop/kill are stable.
