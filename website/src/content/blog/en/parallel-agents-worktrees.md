---
title: How to run multiple AI coding agents in parallel using Git worktrees
metaTitle: Parallel agents with Git worktrees
description: "Run several coding agents at once without them overwriting each other: one git worktree per task, a small script, or a board that sets them up for you."
answer: "Give each agent its own git worktree: a separate folder of the same repository, on its own branch, so agents don't edit the same files. You can create worktrees by hand with git worktree add, wrap that in a small script, or use a board like Coppice, which creates a branch and a worktree for every ticket and shows all the runs in one place. Either way, you review and merge each branch."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - parallel agents
  - coding agents
  - Claude Code
---

## Why it's hard

Two agents in the same folder edit the same files, and one breaks the other's work. Git worktrees fix that: each agent gets its own folder and branch. Keeping those folders apart is one part of [multi-agent coding](/blog/multi-agent-coding).

Then the hard part moves. You have one terminal per agent, you have to remember which one is done and which one is waiting for you, and you merge the branches one by one. Two tasks that touch the same files can still conflict when you merge them.

## Option 1: Create worktrees by hand

Create one worktree per task:

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

Open a terminal in each folder and start an agent there. When a task is done, review and merge its branch, then remove the folder with `git worktree remove`.

This works well for two or three tasks. Beyond that, switching terminals and tracking each task becomes the work.

## Option 2: Wrap it in a small script

A shell function can create the worktree and start the agent in one step:

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

Run `agent_task login` from your project folder. Replace `claude` with `codex` or another agent CLI.

This saves the setup typing. You still have one terminal per task, and tracking and merging are still up to you.

## Option 3: One board for all tasks, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Each ticket gets its own branch and worktree when work starts, so your main checkout isn't touched. Several tickets can run at once, and the board shows which step each one is at.

1. [Install Coppice](/docs/install) and add your project.
2. Create your agents, and pick Claude Code, Codex, or another CLI for each one.
3. Write one ticket per task, and click **Run agent** on each.
4. Approve each plan, then review each diff when it's ready.
5. Merge the tickets one at a time. If a merge conflicts, Coppice lists the files, and you can ask an agent to resolve them.

![A ticket's Git actions panel showing its own branch, agent/TICKET-723e8f0a, and its worktree folder.](/blog/parallel-agents-worktrees/branch-and-worktree.png "Each ticket gets its own branch and worktree.")

[Concepts](/docs/concepts) explains each step a ticket goes through.

## Split tasks so they merge cleanly

Whichever option you use:

- Give each task its own files. Shared files such as lockfiles, schemas, or a shared module belong to one task, or to a later one.
- Merge one branch, update the next one onto the result, and check that the two changes still make sense together. Git can merge two files cleanly even when their content contradicts each other.

## Which one to pick

- **Two or three tasks:** create worktrees by hand.
- **You start tasks often and like the terminal:** a small script.
- **Several tasks at once, and you want one place to see them, review, and merge:** Coppice. Start with [Getting started](/docs/getting-started).
