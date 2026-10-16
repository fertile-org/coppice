---
layout: ../../layouts/Docs.astro
title: Chat with agents
description: "Talk to any agent like a teammate who knows all your projects."
---

# Chat with agents

Open **Chat** and click an agent. There's nothing to set up. Your first message starts the chat.

Each agent can see every repository you've linked, plus your boards, tickets, and Knowledge. Ask about your code, talk an idea through, or have it draft a ticket. Nothing is created until you confirm. It can also suggest a Knowledge note, which goes to your inbox for approval.

Clicking an agent opens its latest chat. Choose **New chat** to start a separate one. An agent with more than one chat shows them all in the sidebar. You can always scroll back through a whole chat.

## How chat reads your code

Each chat has its own folder. When the agent needs a repository, it opens a copy there, so it never works in your checkout.

## Which CLIs can chat

| CLI | Chat |
| --- | --- |
| Claude Code | On, with a banner |
| Codex | On, with a banner |
| OpenCode | On |
| Cursor | Off for now |
| Kilo Code | Off for now |

Coppice hasn't confirmed that Claude Code or Codex stay read-only, so their chats show a banner. The agent can still run commands on your computer.

OpenCode chats use OpenCode's read-only rules. Plugins or MCP tools you've added to OpenCode may get around those rules.

Chat is off for Cursor and Kilo Code until Coppice can keep them read-only.
