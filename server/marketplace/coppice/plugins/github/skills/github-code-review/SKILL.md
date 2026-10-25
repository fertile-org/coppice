---
name: github-code-review
description: Read a GitHub pull request review thread and leave review comments through the github__* tools. Load when a task asks to review a PR, answer review comments, or resolve feedback.
---

# GitHub code review

Use the `github__*` tools exposed by the Coppice MCP gateway. Check `tools/list` for the exact names before calling.

## Reviewing

- Read the full diff and the existing review threads before adding comments.
- Comment on specific lines with a concrete reason: a bug, a missing test, or a contract change. Skip style nits the project does not enforce.
- Submit one review with a clear verdict: approve, request changes, or comment. Do not approve work you did not read.

## Responding to feedback

- Reply in the thread where the comment was made.
- Mark a thread resolved only after the code change it asked for is pushed, or after you explain why it does not apply.

## Report

Summarize the verdict, the blocking points, and the threads you replied to.
