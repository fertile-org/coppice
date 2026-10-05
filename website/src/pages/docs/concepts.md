---
layout: ../../layouts/Docs.astro
title: Concepts
description: "Board, gates, worktrees, and how agents work tickets."
---

# Concepts

## The board
Coppice is a Trello-like board for coding work. Tickets move through stages such as Backlog → Ready → Plan Review → In Progress → In Review → In QA → Wait for Final Review → Done (and Blocked when stuck). Everything important about a ticket lives on the card: comments, run logs, and a live terminal.

## You are the manager
You create or approve tickets, check that the agent understood you at planning, and review the code before work is done. Agents plan and build; you approve and review.

## Human gates
1. **Nothing starts without a human action.** Agents may propose tickets; approving one counts. Approved tickets land in Backlog.
2. **Plan Review.** Drag Ready → a PM agent writes a detailed plan → Plan Review. You approve or ask for changes in ticket comments. Trivial tickets may skip.
3. **Agents execute** through In Progress → In Review → In QA after the plan is approved.
4. **Final Review.** You open the in-app diff, leave feedback as ticket comments, and only then mark done. Approval is tied to what you reviewed.

## Isolated worktrees
When an agent works a ticket, Coppice gives it an isolated git worktree. Agents don’t thrash your main checkout; you review a concrete diff.

## BYO agent CLIs
Coppice does not replace your coding agents. It runs the CLIs you already pay for — Claude Code, Codex, OpenCode, Cursor CLI, Gemini CLI, and similar — against your tickets. See [Providers](/docs/providers).

## Visible collaboration
Ticket comments are the official channel. There are no hidden agent-to-agent side channels. If it happened, you can read it on the card.

## Knowledge hygiene
Agents can store typed, scoped project knowledge for later tickets. Human approval keeps memory useful instead of noisy. Coppice is not “set and forget” memory magic.
