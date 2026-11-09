---
title: How to plan with AI coding agents before they write code
metaTitle: How to plan with AI coding agents
description: "Get a short plan from your coding agent before it edits code: what a good plan has, and three ways to do it, from plan mode to a plan step you approve."
answer: "Ask the agent for a short plan before it edits any code: the approach, the steps, and how the change will be tested. Let it ask questions first, and compare options when there is a real choice. You can do this with a CLI's plan mode, with a plan file in your repository, or on a board like Coppice, where the plan is a separate step you approve and the code is later reviewed against it."
published: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - AI coding agents
  - planning
  - Claude Code
  - Codex
---

## Why plan first

When an agent goes straight to code, a misunderstanding shows up only in the diff. By then the agent has built the wrong thing, and you spend your time reading it, explaining what you meant, and waiting for another attempt. A plan shows the same misunderstanding in a few lines you can read in a minute.

Getting a plan is easy: any agent writes one if you ask. Making it useful is the hard part. The request is vague, the agent guesses instead of asking, the plan lists steps without the decisions behind them, and once coding starts, nobody checks the code against it.

## What a good plan has

- **The outcome and the constraints, in your words.** "Settings gets a dark mode toggle that follows the same theme setting as the rest of the app."
- **Answers, not guesses.** The agent asks what it needs to know before it plans.
- **The approach, and the choice behind it.** When there is a real choice, such as a layout or a data model, the agent shows the options and recommends one.
- **A few steps, including the test** that shows the change works.
- **What it won't change.**

If you can't read the plan in a minute or two, the task is probably too big. Split it.

## Option 1: Plan mode in the CLI

Claude Code has a plan mode: the agent reads the code and proposes a plan without editing files. With other agents, ask for a plan and tell the agent not to edit anything until you approve it.

This is the fastest way to start. The limit is that the plan lives in the conversation. When the session ends, the plan is gone, and you can't easily check the final diff against it.

## Option 2: A plan file in your repository

Ask the agent to write the plan to a Markdown file, such as `docs/plans/dark-mode.md`. Edit the file yourself, then start the implementation with "Implement docs/plans/dark-mode.md", in the same agent or a different one.

The plan survives between sessions and agents, and you can review it like code. You still manage the files, decide when a plan is approved, and compare the diff with the plan by hand.

## Option 3: A plan step you approve, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Every ticket starts with a plan. A PM agent can ask you up to three questions first, then writes the plan. For UI changes, it also shows design options you can compare and choose from. The planning run doesn't change your code. Once you approve the plan, it is saved on the ticket, the agents build from it, and you review the diff next to it.

1. [Install Coppice](/docs/install) and add your project.
2. Write a ticket with the outcome and the constraints, then click **Run agent**.
3. Answer the PM agent's questions, if it asks any.
4. Read the plan. For a UI change, open the design options and choose one. Then click **Approve plan**, or **Ask for changes**.
5. When the code is ready, review the diff against the approved plan.

For a ticket that asks for a dark mode toggle in Settings, the plan names the approach and three steps, including a settings test:

![A ticket titled Add a dark mode toggle to Settings in Plan Review. The proposed plan builds a switch in the Settings row and lists three steps: add the Settings toggle, use the existing theme hook, and add a settings test.](/blog/planning-with-ai-coding-agents/plan.png "The plan names the approach and the steps before any code is written.")

The same ticket comes with three layouts to compare. The PM agent recommends one, and choosing another updates the plan:

![Design options for the dark mode toggle: A, a switch in the Settings row, marked Recommended; B, Light, Dark and System buttons, shown in a live preview; and C, theme preview cards.](/blog/planning-with-ai-coding-agents/design-options.png "Compare the layouts before the agent builds one.")

[Concepts](/docs/concepts) explains each step a ticket goes through.

## Which one to pick

- **A small task in one session:** plan mode.
- **A larger task that spans sessions or agents:** a plan file.
- **Many tasks, with the plan as a step you approve before any code is written:** Coppice. Start with [Getting started](/docs/getting-started).
