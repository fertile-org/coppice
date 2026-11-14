---
title: How to do multi-agent coding without losing control
metaTitle: Multi-agent coding without losing control
description: "Run more than one coding agent without losing the plan or the diff: terminals and worktrees, subagents in one CLI, or a desktop board you approve."
answer: "Give each agent its own folder and branch, and keep a person on the plan and the diff. You can do that with several terminals and git worktrees, with a subagent inside one CLI, or with a desktop board like Coppice, where you approve a plan before code is written and review the diff before it merges."
published: "2026-11-14"
author: Coppice
authorType: Organization
draft: false
keywords:
  - multi-agent coding
  - multi-agent development
  - coding agents
  - Claude Code
  - Codex
---

## Why it's hard

Multi-agent coding, also called multi-agent development, means more than one coding agent on the same project. One might plan, another might implement, and a third might review. It gets messy when you cannot tell who changed what, or when two agents edit the same files at once.

Control is the other problem. Agents write faster than you can read, and several do that in parallel. A short request can come back as a large diff whose summary says everything is fine. If you only read the summary, you have given up the final say.

## Option 1: Several terminals, one folder

Open the project in two or three terminals. Run [Claude Code](/blog/claude-code-and-codex) in one and Codex in another, or run the same CLI twice.

This is enough when the agents take turns. It falls apart when they work at the same time: they share one checkout, so one can overwrite the other's edits, and the next terminal does not see the request.

## Option 2: A git worktree per agent

A [git worktree](/blog/parallel-agents-worktrees) is another folder of the same repository, on its own branch. Create one per task:

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

Start an agent in each folder so they do not edit the same files. When a task is done, review that branch and merge it, then remove the folder with `git worktree remove`.

This runs agents in parallel without a board. You still track one terminal per agent and merge by hand. Two tasks that touch a shared file, such as a lockfile, can conflict when the branches come together.

## Option 3: A subagent inside one CLI

In Claude Code, a subagent is a short file the main session can call. You stay in one terminal, and that helper can use its own model.

That helps when one agent should plan and another should type. It does not give the helper its own folder. Unless you also create a worktree, it edits the same checkout as the session that called it, and another terminal does not inherit the chat.

## Option 4: One board, with Coppice

[Coppice](/) is a desktop app for multi-agent coding. It runs the agent CLIs already on your machine, such as Claude Code, Codex, and Cursor, plus the others listed in [Providers](/docs/providers). Each agent has a role. Several can run at once, each on a ticket.

You stay in control in two places. A PM agent writes a plan, and you approve it before any code is written. When the work comes back, you read the diff. Accepting the work does not merge it. [Planning](/blog/planning-with-ai-coding-agents) and [review](/blog/review-before-merging) are those two checks. A repeated mistake can be saved as a note the next matching ticket is told to read, as described in [stop repeating mistakes](/blog/stop-repeating-mistakes).

1. [Install Coppice](/docs/install) and add your project.
2. Create an agent for each role, and pick the CLI it should run.
3. Write one ticket per task and start the agent. The PM agent writes the plan.
4. Read the plan. Approve it, or ask for changes, before any code is written.
5. When the ticket is ready for you, review the diff. Comment on a line or accept the work, and merge when you are ready.

Each ticket gets its own branch and worktree when work starts, so your main checkout is left alone.

![New agent form with the PM preset selected and Claude Code chosen as the agent's CLI.](/blog/claude-code-and-codex/new-agent.png "Each agent runs on one CLI. Pick the CLI when you create the agent.")

![A ticket's Git actions panel showing its own branch, agent/TICKET-723e8f0a, and its worktree folder.](/blog/parallel-agents-worktrees/branch-and-worktree.png "Each ticket gets its own branch and worktree.")

[Getting started](/docs/getting-started) and [Concepts](/docs/concepts) cover the first ticket and the steps it goes through.

## Which one to pick

- **Agents take turns, and you are fine watching two terminals:** one folder is enough.
- **A few tasks at once, and you like the terminal:** one git worktree per task.
- **One session should split plan and code:** a subagent in that CLI, plus a worktree if they must not share files.
- **Many tasks, and you want one place to approve the plan and the diff:** Coppice.
