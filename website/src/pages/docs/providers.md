---
layout: ../../layouts/Docs.astro
title: Providers
description: "Which agent CLIs Coppice talks to."
---

# Providers

**Connect your agent CLI.** Pick your agent CLI when you create an agent, and Coppice turns it on. Each one shows whether it's **Ready**, **Found, not signed in**, or **Not on your PATH**. Sign in to the CLI in your terminal first, the same way you normally would.

Coppice can't check Kilo Code's sign-in, so it shows no status. Make sure you're signed in to it in your terminal.

Coppice talks to coding agent CLIs you install yourself. Bring the tools you already use; Coppice does not sell model access.

## Supported (Beta)
Exact wiring may expand — check the app’s provider settings for the live list.

| Provider | Notes |
| --- | --- |
| Claude Code | Anthropic’s agent CLI |
| Codex | OpenAI’s agent CLI |
| Cursor | Cursor’s CLI agent |
| OpenCode | Open-source coding agent |
| Kilo Code | Kilo’s agent CLI |

## How connection works
1. Install and authenticate the CLI the normal way for that tool.
2. In Coppice, pick the provider for an agent role on your board.
3. When a ticket is ready, Coppice starts that CLI in an isolated worktree and streams output to the ticket’s live terminal.

## What Coppice does not do
- It does not run a cloud coding service
- It does not merge code without your Human Review
