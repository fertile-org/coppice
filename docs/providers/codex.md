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

## Why deferred

Harder to control than attach/API providers. Document first; implement when `TmuxStream` and stop/kill are stable.
