---
layout: ../../layouts/Docs.astro
title: Providers
metaTitle: "Supported Agent CLIs: Claude Code, Codex, Cursor"
description: "Coppice runs the agent CLIs you already use: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code and Grok."
---

# Providers

Coppice runs the coding agent CLIs you already use and puts their work on one board. You keep your own subscriptions and sign-ins. You don't need a Coppice account.

Coppice does not install these CLIs. Install the command from the vendor guide in the table. Then sign in with the CLI in your terminal, before a run. That sign-in stays with the CLI. Coppice never asks for a provider API key and does not store one.

## Supported connectors

| Connector | Command Coppice looks for | Install | Tested versions |
| --- | --- | --- | --- |
| Claude Code | `claude` | [Install guide](https://docs.anthropic.com/en/docs/claude-code/setup) | 2.1.309 |
| Codex | `codex` | [Install guide](https://developers.openai.com/codex/cli) | 0.163.0 |
| Cursor | `cursor-agent` | [Install guide](https://cursor.com/docs/cli/installation) | 2026.10.01 |
| OpenCode | `opencode` | [Install guide](https://opencode.ai/docs/) | 1.x, 2.x |
| Kilo Code | `kilo` | [Install guide](https://kilo.ai/docs/cli) | 7.8.3 |
| GitHub Copilot | `copilot` | [Install guide](https://docs.github.com/en/copilot/how-tos/copilot-cli) | 1.0.98 |
| Qwen Code | `qwen` | [Install guide](https://github.com/QwenLM/qwen-code) | 0.25.2 |
| Kimi Code | `kimi` | [Install guide](https://moonshotai.github.io/kimi-code/) | 2.2.0 |
| Grok | `grok` | [Install guide](https://x.ai/cli) | 1.0.62 |

We will update this list as we test more CLI versions.

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="The model picker on an agent, listing the models from the agent's CLI.">
<figcaption>Pick a model, or leave it on the default.</figcaption>
</figure>

Pick a model for this agent, or leave it on the default. If you installed a new model, click **Refresh models**.

## Connect your agent CLI

1. Install the CLI from the guide in the table above. Coppice does not install it for you.
2. Sign in to that CLI in your terminal, the same way you normally would. Do this before a ticket runs. Coppice never asks for or stores the provider API key.
3. Pick your agent CLI when you create an agent, and Coppice turns it on.

The agent form lists every connector with its status:

| Status | What it means |
| --- | --- |
| **Ready** | A free sign-in check confirmed you are signed in. |
| **Found, not signed in** | Coppice found the command, but you aren't signed in. Sign in from your terminal. |
| **Not on your PATH** | Coppice can't find the command. Install it, or check your PATH. |
| **Installed** | Coppice found the command. For Kilo Code, GitHub Copilot, Qwen Code and Kimi Code it cannot check sign-in without using your plan. |

Claude Code, Codex, Cursor, OpenCode, and Grok have a free sign-in check, so a clear result shows **Ready** or **Found, not signed in**. The check does not open a browser, prompt you, or call a model.

Kilo Code, GitHub Copilot, Qwen Code, and Kimi Code show **Installed**. System → Connectors adds: "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out." To check one of these CLIs, use **Test connection**. It runs a short chat, plan, and ticket turn.

Every CLI can chat. Claude Code and Kilo Code chats show "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer." OpenCode, Codex, Cursor, GitHub Copilot, Qwen Code, Kimi Code, and Grok chats have no banner. Coppice blocks their file writes and shell commands in chat and planning. A tool from your own MCP setup in that CLI can still change things.

You can save an agent before its CLI is ready. The agent starts working tickets once the CLI is installed and signed in. If you installed the CLI after opening Coppice, quit and reopen Coppice so it finds the new command.

If Coppice finds the command, the connector is on, unless you aren't signed in. To turn one on or off, use its switch in **System → Connectors**.

## Use your own API key or another provider

To use your own API key, or a model your CLI doesn't offer, use OpenCode. It works with 75+ model providers and with local models. See [OpenCode's provider list](https://opencode.ai/docs/providers/).

1. Install OpenCode.
2. Run `opencode` in your terminal, type `/connect`, pick your provider and paste your key. OpenCode keeps the key, and Coppice never asks for it. Not in the list? Follow OpenCode's [custom provider steps](https://opencode.ai/docs/providers/#custom-provider).
3. In Coppice, create an agent and set **Connector** to **OpenCode**. Your provider's models show up under **Model**. If you added the provider while Coppice was open, click **Refresh models**.

## On a ticket

When an agent picks up a ticket, Coppice starts its CLI in that ticket's own git worktree. You can watch its progress live on the ticket.

## What Coppice does not do
- It does not run a cloud coding service
- It does not merge code without your Human Review
