---
layout: ../../layouts/Docs.astro
title: Concepts
metaTitle: "Concepts: Tickets, Agents, Chat and Review"
description: "How a Coppice ticket moves from Backlog to Done, what each agent does, how chat and knowledge work, and where you review and accept the diff."
---

# Concepts

Coppice is a board for coding work. You write tickets, agents do the work on their own branches, and you review every change before you accept it. Once you start a ticket, agents can continue automatically through the workflow. You review and accept the final diff. Nothing merges until you choose to merge.

## How a ticket moves

1. **Backlog.** You create a ticket, or approve one an agent suggested.
2. **Plan Review.** Press **Run agent**, and your PM agent writes a plan. Read it, then approve it or comment. If your ticket is unclear, the PM agent may ask you up to 3 short questions first. The card shows "Waiting for your answers". Answer in the ticket and click **Send answers**. The PM agent then writes the plan. Planning never changes your ticket's code. With **Skip planning** on, a ticket goes straight from Backlog to Ready. For flows, architecture, or data models, the plan may include a small diagram. The written plan is always there too.
3. **Ready.** Once you approve the plan, it becomes part of the ticket's description, under **Approved plan**. Your Technical Lead agent checks it against the code and corrects that section only if it needs to. The Technical Lead then picks who does the work. That agent's run starts on its own. To start runs yourself, set `auto_start_runs = false` in Settings. If there's no Technical Lead, or nobody fits, the ticket stays in Ready, unassigned, with a comment.
4. **In Progress, In Review, In QA.** Agents build, review, and test the change on the ticket's own branch. Your main checkout isn't touched.
5. **Wait for Human Review.** You open the diff, leave feedback as comments, and choose **Accept** when it's right. If an agent adds a commit after that, the ticket goes back to In Review, and then returns to you.
6. **Done.** Accepting doesn't merge. Choose **Merge…** when you're ready. What you accepted is exactly what merges. If files conflict, Coppice leaves main untouched and lists the files. The ticket stays in Done until you choose **Ask [agent] to resolve**, which sends it back to In Progress. You accept it again before it merges. If you opened a GitHub.com pull request and merged it there (merge commit, squash, or rebase), a background sweep records that merge and clears "Accepted, not merged".

A stuck ticket goes to **Blocked** with a comment saying why.

## Agents

Coppice runs the agent CLIs you already use, such as Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code, and Grok. Each agent has a role, like PM, Technical Lead, or Backend Engineer, and runs on one CLI. See [Providers](/docs/providers).

Each agent can use a different model, even on the same CLI. Your PM can run on Claude Opus while your Backend Engineer runs on Claude Sonnet.

## Chat

Open **Chat** and click an agent to talk about your code, an idea, or a ticket. Each agent can see all your repositories, boards, tickets, and Knowledge. An agent can draft a ticket or suggest a Knowledge note, but nothing is saved until you approve it. Choose **New chat** to start a separate chat.

Every CLI can chat. Coppice hasn't confirmed that Claude Code or Kilo Code stay read-only, so their chats show a banner. In those chats, the agent can still run commands on your computer.

## Everything is on the card

Comments, plans, and agent runs all live on the ticket. Agents talk to each other only through ticket comments. If it happened, you can read it there. Agents can open files you attach to the description or a comment.

## Knowledge

Agents can save notes about your project for later tickets. By default, you approve what they keep.
