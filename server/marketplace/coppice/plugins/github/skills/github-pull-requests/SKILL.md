---
name: github-pull-requests
description: Open, inspect and update GitHub pull requests through the github__* tools. Load when a task asks to create a PR, read its diff or status, or update its title or body.
---

# GitHub pull requests

Use the `github__*` tools exposed by the Coppice MCP gateway. Check `tools/list` for the exact names before calling.

## Before opening a PR

- Push only when the task and the project's git rules allow it, and only the branch you were asked to work on.
- Base the PR on the repository's default branch unless the ticket names another base.
- Write the title and body from the commits. Name the ticket, what changed, and how it was verified.

## Inspecting a PR

- Read the description, changed files and review state before commenting.
- Check the status checks. A failing check is a fact to report, not something to hide.

## Updating a PR

- Change only the title or body the task asks for.
- Never merge a PR unless the task explicitly says to and the project's merge gate allows it.

## Report

Return the PR URL, the base and head branches, and the status of its checks.
