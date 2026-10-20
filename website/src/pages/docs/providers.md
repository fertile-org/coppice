---
layout: ../../layouts/Docs.astro
title: Providers
description: "The agent CLIs Coppice runs: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code and Grok."
---

# Providers

Tired of keeping a terminal open for every agent? Coppice runs the coding agent CLIs you already use and puts their work on one board. You keep your own subscriptions and sign-ins. No Coppice account.

## Supported connectors

| Connector | Command Coppice looks for | Install |
| --- | --- | --- |
| Claude Code | `claude` | [Install guide](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [Install guide](https://developers.openai.com/codex/cli) |
| Cursor | `cursor-agent` | [Install guide](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [Install guide](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [Install guide](https://kilo.ai/docs/cli) |
| GitHub Copilot | `copilot` | [Install guide](https://docs.github.com/en/copilot/how-tos/copilot-cli) |
| Qwen Code | `qwen` | [Install guide](https://github.com/QwenLM/qwen-code) |
| Kimi Code | `kimi` | [Install guide](https://moonshotai.github.io/kimi-code/) |
| Grok | `grok` | [Install guide](https://x.ai/cli) |

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="The model picker on an agent, listing the models from the agent's CLI.">
<figcaption>Pick a model, or leave it on the default.</figcaption>
</figure>

Pick a model for this agent, or leave it on the default. Installed a new model? Click Refresh models.

## Connect your agent CLI

1. Install the CLI and sign in to it in your terminal, the same way you normally would.
2. Pick your agent CLI when you create an agent, and Coppice turns it on.

The agent form lists every connector with its status:

| Status | What it means |
| --- | --- |
| **Ready** | A free sign-in check confirmed you are signed in. |
| **Found, not signed in** | Coppice found the command, but you aren't signed in. Sign in from your terminal. |
| **Not on your PATH** | Coppice can't find the command. Install it, or check your PATH. |
| **Installed** | Coppice found the command. For Kilo Code, GitHub Copilot, Qwen Code, Kimi Code and Grok it cannot check sign-in without using your plan. |

Kilo Code, GitHub Copilot, Qwen Code, Kimi Code and Grok show **Installed**. System → Connectors adds: "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out." To see that one works, use **Test connection**: it runs a short chat, plan and ticket turn. Every CLI can chat. Claude Code, Codex, Cursor, and Kilo Code chats show "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer." OpenCode, GitHub Copilot, Qwen Code, Kimi Code and Grok chats have no banner: Coppice blocks their file writes and shell commands in chat and planning. A tool from your own MCP setup in that CLI can still change things.

You can save an agent before its CLI is ready. It starts working tickets once the CLI is installed and signed in. If you installed the CLI after opening Coppice, quit and reopen Coppice so it finds the new command.

To turn a connector off, use **System → Connectors**.

## Use your own API key or another provider

Want to use your own API key, or a model your CLI doesn't offer? Use OpenCode. It works with 75+ model providers and with local models. See [OpenCode's provider list](https://opencode.ai/docs/providers/).

1. Install OpenCode.
2. Run `opencode` in your terminal, type `/connect`, pick your provider and paste your key. OpenCode keeps the key, and Coppice never asks for it. Not in the list? Follow OpenCode's [custom provider steps](https://opencode.ai/docs/providers/#custom-provider).
3. In Coppice, create an agent and set **Connector** to **OpenCode**. Your provider's models show up under **Model**. If you added the provider while Coppice was open, click **Refresh models**.

## On a ticket

When an agent picks up a ticket, Coppice starts its CLI in that ticket's own git worktree. You can watch its progress live on the ticket.

## What Coppice does not do
- It does not run a cloud coding service
- It does not merge code without your Human Review
