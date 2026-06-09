# Shell provider

**ID:** `shell`  
**Status:** Deferred  
**Stream backend:** `TmuxStream` or direct exec (TBD)

Generic wrapper for a custom shell command configured per agent or globally.

## Use case

Run a user-defined script or binary instead of a named coding-agent CLI, while still fitting Coppice's worktree + result contract pipeline.

## Why deferred

Needs a clear config schema (command, args, env) and the same streaming/stop primitives as other CLI providers. Lower priority than OpenCode, Claude Code, and Codex.
