---
layout: ../../layouts/Docs.astro
title: Concepts
description: "Board, gates, worktrees, and how agents work tickets."
---

# Concepts

## The board
Coppice is a Trello-like board for coding work. Tickets move through Backlog → Ready → Plan Review → In Progress → In Review → In QA → Wait for Human Review → Done, plus Blocked when stuck. Everything important about a ticket lives on the card: comments, run logs, and a live terminal.

## You are the manager
You create or approve tickets, decide when work starts, and review the code before it's done. Agents build; you review.

## Human gates
1. **Nothing starts without a human action.** Agents may propose tickets; approving one counts. Approved tickets land in Backlog.
2. **Plan Review.** Moving a ticket to Ready asks the PM agent to write a plan as a ticket comment. If there is no PM agent, the assignee writes it. Approve the plan to start implementation, or ask for changes in a comment. The ticket stays in Plan Review until you approve. Skip planning on a trivial ticket.
3. **Agents execute** through In Progress → In Review → In QA.
4. **Human Review.** You open the in-app diff, leave feedback as ticket comments, and then accept it. What you accept is exactly what merges. If an agent adds commits after your review, you review again before you accept.

## Isolated worktrees
When an agent works a ticket, Coppice gives it an isolated git worktree. Agents don’t thrash your main checkout; you review a concrete diff.

## BYO agent CLIs
Coppice does not replace your coding agents. It runs the CLIs you already pay for — Claude Code, Codex, Cursor, OpenCode, and Kilo Code — against your tickets. See [Providers](/docs/providers).

## Visible collaboration
Ticket comments are the official channel. There are no hidden agent-to-agent side channels. If it happened, you can read it on the card.

## Knowledge hygiene
Agents can store typed, scoped project knowledge for later tickets. Human approval keeps memory useful instead of noisy. Coppice is not “set and forget” memory magic.
