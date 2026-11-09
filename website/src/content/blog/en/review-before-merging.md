---
title: How to review AI-generated code before merging
description: "What to check in an AI agent's diff before you merge it, and three ways to review agent code: in git, with a second agent, or with a plan you approve first."
answer: "Check the diff against what you asked for: files it should not touch, error handling, new dependencies, and tests that really cover the change. You can review in git or a pull request, ask a second agent for a first pass, or use a board like Coppice, where you approve a plan before any code is written and review the diff against that plan before it merges."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - AI code review
  - coding agents
  - code review
  - merge
---

## Why it's hard

Agents write code faster than you can read it. A small request can come back as a large diff, with changes you didn't ask for, and a summary that says everything works. Reading every line takes too long. Skimming lets mistakes through.

The hard part is knowing what the agent was supposed to do. Without that, you can't tell whether a change belongs in the diff or is scope nobody asked for.

## What to check in the diff

Whichever way you review, look for these:

- **Scope.** Files you didn't expect, such as lockfiles, migrations, config, or renames.
- **Error paths.** What happens when input is missing, a call fails, or a value is empty.
- **Dependencies and access.** New packages, scripts, credentials, or changes to who can read or change data.
- **Tests.** Tests that exercise the change, not only a line in the summary saying they pass.

## Option 1: Review in git or a pull request

Diff the agent's branch against your main branch, or open a pull request and review it the way you would review a teammate's work. This works with any agent and any tool you already use.

The limit is that you only see the result. What you asked for lives in a chat history, so you compare the diff against your memory of the request.

## Option 2: Ask a second agent to review first

Ask a different agent to review the diff before you do, for example Codex reviewing what Claude Code wrote. It catches obvious problems and gives you a list of places to look.

Its review is a second opinion, not a sign-off. You still read the diff and decide.

## Option 3: Approve the plan first, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Before any code is written, an agent writes a plan and you approve it. When the work is done, Coppice shows the diff next to the plan you approved and tags any file the plan didn't mention. Accepting the work does not merge it, so you choose when it reaches your main branch.

1. [Install Coppice](/docs/install) and add your project.
2. Write a ticket and click **Run agent**. A PM agent writes the plan.
3. Read the plan. Click **Approve plan**, or **Ask for changes**.
4. When the ticket reaches **Wait for Human Review**, click **Review code**. Comment on lines, or accept the work.
5. Click **Merge…** when you're ready.

![A ticket in Plan Review. The proposed plan says to add docs/greeting.md, with Approve plan and Ask for changes below it.](/blog/review-before-merging/plan-review.png "You approve the plan before any code is written.")

![Code review page with the changed files on the left and the diff for docs/greeting.md on the right.](/blog/review-before-merging/diff-before-accept.png "The diff you review before you accept the work.")

[Concepts](/docs/concepts) explains each step a ticket goes through.

## Which one to pick

- **A small change from one agent:** review it in git or a pull request.
- **A larger change, and you want a first pass:** ask a second agent, then read it yourself.
- **Many agent tasks, and you want to agree on the plan before code is written:** Coppice. Start with [Getting started](/docs/getting-started).
