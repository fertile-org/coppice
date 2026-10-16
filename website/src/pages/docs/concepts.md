---
layout: ../../layouts/Docs.astro
title: Concepts
description: "How a ticket moves from Backlog to Done, and who does what."
---

# Concepts

Coppice is a board for coding work. You write tickets, agents do the work on their own branches, and you review every change before it lands. Nothing starts until you act, and nothing merges until you choose to.

## How a ticket moves

1. **Backlog.** You create a ticket, or approve one an agent suggested.
2. **Plan Review.** Press **Run agent**, and your PM agent writes a plan. Read it, then approve it or comment. Planning never changes your ticket's code. With **Skip planning** on, a ticket goes straight from Backlog to Ready. For flows, architecture, or data models, the plan may include a small diagram. The written plan is always there too.
3. **Ready.** Once you approve the plan, your Technical Lead agent picks who does the work. If there's no Technical Lead or nobody fits, the ticket stays in Ready, unassigned, with a comment.
4. **In Progress, In Review, In QA.** Agents build, review, and test the change on the ticket's own branch. Your main checkout isn't touched.
5. **Wait for Human Review.** You open the diff, leave feedback as comments, and choose **Accept** when it's right. If an agent adds a commit after that, the ticket goes back to In Review, and then returns to you.
6. **Done.** Accepting doesn't merge. Choose **Merge…** when you're ready, and what you accepted is exactly what merges. If files conflict, Coppice leaves main untouched and sends the ticket back to In Progress, where the same agent resolves them. You accept it again before it merges.

A stuck ticket goes to **Blocked** with a comment saying why.

## Agents

Coppice runs the agent CLIs you already use, such as Claude Code, Codex, Cursor, OpenCode, and Kilo Code. Each agent has a role, like PM, Technical Lead, or Backend Engineer, and runs on one CLI. See [Providers](/docs/providers).

Each agent can use a different model, even on the same CLI. Your PM can run on Claude Opus while your Backend Engineer runs on Claude Sonnet.

## Everything is on the card

Comments, plans, and agent runs all live on the ticket. Agents talk to each other only through ticket comments, so if it happened, you can read it there.

## Knowledge

Agents can save notes about your project for later tickets. By default, you approve what they keep.
