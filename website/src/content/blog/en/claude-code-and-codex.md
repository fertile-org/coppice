---
title: How to use Claude Code and Codex together on one project
metaTitle: Claude Code and Codex on one project
description: "Three ways to run Claude Code and Codex on the same codebase: two terminals, git worktrees, or one board where each agent takes its own tickets."
answer: "Give each tool its own copy of the code so they don't edit the same files at once. You can do that by hand with git worktrees, or use a board like Coppice, where every ticket gets its own branch and you choose whether Claude Code or Codex works on it. Either way, you review the changes before they reach your main branch."
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - coding agents
  - git worktree
---

## Why use both

Claude Code and Codex are both coding agents that run in your terminal. You might want one to plan and the other to write the code, or you might want a second model to review what the first one changed. The hard part is not running them. It is keeping their changes apart so you can tell who changed what.

There are three common ways to set this up. Two tools on one project is a small case of [multi-agent coding](/blog/multi-agent-coding).

## Option 1: Two terminals, one checkout

Open your project in two terminals. Run `claude` in one and `codex` in the other.

This is the quickest way to start. It works well when you use one tool at a time, for example using Codex to review what Claude Code just wrote. If both tools edit at once, they can change the same file, and one can overwrite the other's work.

## Option 2: One git worktree per tool

A git worktree is a second working folder for the same repository, on its own branch. Create one for Codex:

```bash
git worktree add ../my-project-codex -b codex-task
```

Run `claude` in your main folder and `codex` in `../my-project-codex`. Both can work at the same time without touching each other's files. When they finish, you review each branch and merge it yourself.

This keeps the work apart, but you still do the coordination: you decide which tool gets which task, pass context between them, and clean up the branches.

## Option 3: One board for both, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. It runs the `claude` and `codex` commands already on your computer. Each ticket runs on its own branch, so your main checkout isn't touched, and agents hand tickets to each other through the board.

1. [Install Coppice](/docs/install). Sign in to Claude Code and Codex in your terminal the way you normally do.
2. Add your project folder under **Repositories**.
3. Create your agents, and pick Claude Code or Codex for each one. For example, a PM and a Technical Lead on Claude Code, a Backend Engineer on Codex, and a Reviewer on Claude Code, so Claude Code reviews what Codex writes.
4. Write a ticket and click **Run agent**.

The PM writes a plan, and you approve it. The agents then build and review the change. You read the final diff, accept it, and merge when you're ready.

![New agent form with the PM preset selected and Claude Code chosen as the agent's CLI.](/blog/claude-code-and-codex/new-agent.png "Each agent runs on one CLI. Pick Claude Code or Codex when you create it.")

## Which one to pick

- **A quick task, one tool at a time:** two terminals.
- **Parallel work, and you're happy to manage branches:** git worktrees.
- **Several tickets, agents that hand off and review each other's work, and you approve the plan and the diff:** Coppice. Start with [Getting started](/docs/getting-started).
