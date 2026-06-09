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

## Why deferred

Subscription CLIs are harder to control than API-key providers. Needs stable tmux capture and stop/kill before implementation.
