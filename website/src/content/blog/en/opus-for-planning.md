---
title: How to use Opus for planning and a cheaper model for coding
metaTitle: Plan with Opus, code with a cheaper model
description: "Use Opus for the plan and a cheaper model for the code. Claude Code can switch with opusplan, a subagent, or a pasted plan. Coppice sets one model per agent."
answer: "Use Opus for the plan and a cheaper model, such as Sonnet, for the code. In Claude Code, opusplan uses Opus in plan mode and Sonnet afterward, a subagent can set its own model, or you can paste an Opus plan into a cheaper session. In Coppice, each agent has its own model, so the PM can plan on Opus and the engineer can build on Sonnet."
published: "2026-11-11"
draft: true
author: Coppice
authorType: Organization
keywords:
  - Opus
  - Sonnet
  - Claude Code
  - coding agents
  - planning
---

## Why this is hard

Opus is a strong planner, and an expensive model to leave on for the whole task. It uses more of your usage limit than Sonnet, and it costs more if you pay by the token. Most of a coding task comes after the plan, when the agent reads files, edits them, and retries. Run that part on Opus and the costly model does the mechanical work. Run the whole task on a cheaper model and the plan is weaker. The hard part is keeping that plan with the coding step.

## Option 1: Claude Code's opusplan setting

In a Claude Code session, run `/model opusplan`.

In plan mode, Claude Code uses Opus. When you leave plan mode, it switches to Sonnet for the code. Outside plan mode, that setting stays on Sonnet, so the Opus step only happens if you plan first.

This is the quickest way in one session. The plan stays in the conversation, and it is gone when the session ends.

## Option 2: A subagent with its own model

A Claude Code subagent is a Markdown file. Its `model` field sets the model for that subagent. Keep the main session on Opus, and put the coding on Sonnet. Save a file under `.claude/agents/`:

```markdown
---
name: implementer
description: Implements an approved plan
model: sonnet
---
```

Ask the session to hand the coding to that subagent. Only the work it delegates runs on Sonnet. If the main session writes the code itself, that part still runs on Opus. You still review the diff.

## Option 3: Switch models, or paste the plan

Plan on Opus, then run `/model sonnet` and tell it to implement the plan. The conversation stays, so Sonnet can read the plan already there.

To start a new session, copy the plan and launch on Sonnet:

```bash
claude --model sonnet
```

Paste the plan in as the task. You can paste it into another agent CLI the same way. The new session only knows what you pasted, and you check the diff against the plan yourself.

## Option 4: One model per agent, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Each agent has its own model, even on the same CLI. For a ticket such as a dark mode toggle in Settings, the PM can plan on Opus and the Frontend Engineer can build on Sonnet.

1. [Install Coppice](/docs/install). Sign in to Claude Code in your terminal the way you normally do, and add your project.
2. Create a PM agent, pick Claude Code, and set the model to Opus.
3. Create a Technical Lead and a Frontend Engineer on Claude Code. Set the Frontend Engineer's model to Sonnet, or to another cheaper model Claude Code offers.
4. Write the ticket and click **Run agent**. The PM writes the plan on Opus. Read it, then click **Approve plan** or **Ask for changes**. The planning run doesn't change your code.
5. When the code is ready, review the diff.

After you approve, the Technical Lead checks the plan and assigns the Frontend Engineer. The Frontend Engineer's run uses the model you set, and it starts on its own. [Concepts](/docs/concepts) explains each step.

![New agent dialog named PM. The model list is open, and Claude Opus 5.5 is selected.](/blog/opus-for-planning/pm-opus.png "The PM's model list, with Claude Opus 5.5 selected.")

![New agent dialog for a Frontend Engineer. Claude Code is chosen and marked Ready, and the model is Claude Sonnet 5.5.](/blog/opus-for-planning/frontend-sonnet.png "The Frontend Engineer uses Claude Code, with Claude Sonnet 5.5 selected.")

## Which one to pick

- **One Claude Code session, and you will enter plan mode:** `opusplan`.
- **A cheaper model for a step you delegate:** a subagent with `model: sonnet`.
- **A plan you want in a new session:** paste it into a session on Sonnet. In the same conversation, `/model sonnet` keeps the plan.
- **Many tickets, with the planner and the coder each on their own model:** Coppice. Start with [Getting started](/docs/getting-started).
