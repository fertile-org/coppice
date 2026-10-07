---
layout: ../../layouts/Docs.astro
title: Providers
description: "The agent CLIs Coppice runs: Claude Code, Codex, Cursor, OpenCode and Kilo Code."
---

# Providers

Tired of keeping a terminal open for every agent? Coppice runs the coding agent CLIs you already use and puts their work on one board. You keep your own subscriptions and sign-ins. No Coppice account.

## Supported connectors

| Connector | Command Coppice looks for | Install |
| --- | --- | --- |
| Claude Code | `claude` | [Install guide](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [Install guide](https://developers.openai.com/codex/cli) |
| Cursor | `agent` | [Install guide](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [Install guide](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [Install guide](https://kilo.ai/docs/cli) |

## Connect your agent CLI

1. Install the CLI and sign in to it in your terminal, the same way you normally would.
2. Pick your agent CLI when you create an agent, and Coppice turns it on.

The agent form lists every connector with its status:

| Status | What it means |
| --- | --- |
| **Ready** | Coppice found the command and your sign-in. |
| **Found, not signed in** | Coppice found the command, but you aren't signed in. Sign in from your terminal. |
| **Not on your PATH** | Coppice can't find the command. Install it, or check your PATH. |

Coppice can find Kilo Code but can't check its sign-in, so it shows no status. Make sure you're signed in to Kilo Code in your terminal.

You can save an agent before its CLI is ready. It starts working tickets once the CLI is installed and signed in. If you installed the CLI after opening Coppice, quit and reopen Coppice so it finds the new command.

To turn a connector off, use **Tools → Connectors**.

## On a ticket

When an agent picks up a ticket, Coppice starts its CLI in that ticket's own git worktree. You can watch its progress live on the ticket.

## What Coppice does not do
- It does not run a cloud coding service
- It does not merge code without your Human Review
