---
layout: ../../layouts/Docs.astro
title: Chat with agents
description: "Ask an agent about your code, and turn the answer into a ticket."
---

# Chat with agents

Open **Chat**, pick an **Agent**, and optionally a board, then choose **Start chat**.

Ask about your code or talk an idea through. The agent can draft a ticket or suggest a Knowledge note, and nothing is created until you confirm. You can attach up to 5 files to each message.

When a chat gets long, choose **Continue in new chat**. A new chat opens with a summary of this one, and you can still read the old one.

## Which CLIs can chat

Chat is meant to read your code, not change it. Each CLI handles that differently:

| CLI | Chat |
| --- | --- |
| Claude Code | On, in a temporary copy of your repo, with a banner |
| Codex | On, in a temporary copy of your repo, with a banner |
| OpenCode | On, in your repo |
| Cursor | Off for now |
| Kilo Code | Off for now |

Coppice hasn't confirmed that Claude Code or Codex stay read-only, so their chats run in a temporary copy of your repo with no remotes. The agent can still run commands on your computer.

OpenCode chats run in your repo with OpenCode's read-only rules. Plugins or MCP tools you've added to OpenCode may get around those rules.

Chat is off for Cursor and Kilo Code until Coppice can keep them read-only.
