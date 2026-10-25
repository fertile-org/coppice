---
name: github-issues
description: Read and update GitHub issues through the github__* tools. Load when a task names an issue, a bug report on GitHub, or asks to open, comment on, label or close one.
---

# GitHub issues

Use the `github__*` tools exposed by the Coppice MCP gateway. Check `tools/list` for the exact names before calling. Do not shell out to a GitHub CLI for issue work.

## Read first

- Fetch the issue, its comments and its labels before changing anything.
- Confirm the repository owner and name from the ticket or the worktree's git remote. Do not guess.

## Write carefully

- Post one comment per update. Say what you checked and what changed.
- Close an issue only when the task says so or a linked pull request has merged.
- Do not change labels, assignees or milestones unless the task asks.

## Report

State the issue URL, what you read, and any write you made. If the token is missing or the tool returns an auth error, report it as blocked. Do not work around it.
