---
layout: ../../layouts/Docs.astro
title: Providers
description: "Which agent CLIs Coppice talks to."
---

# Providers

Coppice talks to coding agent CLIs you install yourself. Bring the tools you already use; Coppice does not sell model access.

## Supported (Beta)
Exact wiring may expand — check the app’s provider settings for the live list.

| Provider | Notes |
| --- | --- |
| Claude Code | Anthropic’s agent CLI |
| Codex | OpenAI’s agent CLI |
| OpenCode | Open-source coding agent |
| Cursor CLI | Cursor’s CLI agent |
| Gemini CLI | Google’s agent CLI |

## How connection works
1. Install and authenticate the CLI the normal way for that tool.
2. In Coppice, pick the provider for an agent role on your board.
3. When a ticket is ready, Coppice starts that CLI in an isolated worktree and streams output to the ticket’s live terminal.

## What Coppice does not do
- It does not run a cloud coding service
- It does not ship a fake or mock agent in end-user builds
- It does not merge code without your Final Review
