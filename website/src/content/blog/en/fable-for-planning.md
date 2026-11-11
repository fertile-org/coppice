---
title: How to use Fable for planning and a cheaper model for coding
metaTitle: Plan with Fable, code with a cheaper model
description: "Use Fable 5.2 for the plan and Opus or Sonnet for the code. Claude Code has no Fable-then-cheaper setting. Coppice can set one model per agent."
answer: "Use Fable 5.2 for the plan and a cheaper model, Opus or Sonnet, for the code. Claude Code has no setting that does this: opusplan is still Opus then Sonnet, so plan with /model fable and switch, or give a subagent its own model. In Coppice, each agent has its own model, so the PM can plan on Fable and the engineer can build on Opus or Sonnet."
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - planning
---

## Why this is hard

Claude Fable 5.2 is Anthropic's model for the hardest, open-ended work. For most coding, they say to start with Opus 5.5, and to use Sonnet 5.5 for a smaller, scoped task. Fable costs more than Opus, and Opus costs more than Sonnet. Most of the tokens come after the plan: reading files, edits, and retries. Leave Fable on for that and it does the mechanical work. You want Fable on the plan and Opus or Sonnet on the code, with the plan still there when coding starts.

## Option 1: Plan on Fable, then switch

Claude Code has no setting that uses Fable for the plan and a cheaper model for the code. `opusplan` is still Opus in plan mode and Sonnet afterward.

Run `/model fable`, then enter plan mode. In the terminal that alias is Fable 5.2. In a Claude apps gateway session it is still Fable 5, so run `/model claude-fable-5-2` there. When the plan is ready, run `/model opus` or `/model sonnet`. The conversation stays. When the session ends, the plan is gone.

## Option 2: A subagent with its own model

A Claude Code subagent is a Markdown file. Its `model` field sets that subagent's model: `fable`, `opus`, `sonnet`, `haiku`, or a full id. Keep the session on Fable and put the coding on Opus or Sonnet. Save this under `.claude/agents/`:

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

Use `sonnet` when the task is small. Only delegated work runs on that model. Code the main session writes itself still runs on Fable.

## Option 3: Paste the plan into a cheaper session

Plan on Fable, copy the plan, and start again on Opus or Sonnet:

```bash
claude --model opus
```

`claude --model sonnet` is the same step for a smaller task. Paste the plan in. You can paste it into another agent CLI the same way. The new session only knows what you pasted.

## Option 4: One model per agent, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Each agent has its own model, even on the same CLI. For a ticket such as a dark mode toggle in Settings, the PM can plan on Fable 5.2 and the Frontend Engineer can build on Opus 5.5 or Sonnet.

1. [Install Coppice](/docs/install). Sign in to Claude Code in your terminal the way you normally do, and add your project.
2. Create a PM agent, pick Claude Code, and set the model to Fable 5.2.
3. Create a Technical Lead and a Frontend Engineer on Claude Code. Set the Frontend Engineer's model to Opus 5.5, or to Sonnet when the change is small.
4. Write the ticket and click **Run agent**. The PM writes the plan on Fable. Read it, then click **Approve plan** or **Ask for changes**. The planning run doesn't change your code.
5. When the code is ready, review the diff.

After you approve, the Technical Lead checks the plan and assigns the Frontend Engineer. The Frontend Engineer's run uses the model you set, and it starts on its own. [Concepts](/docs/concepts) explains each step.

![New agent dialog named PM. The model list is open, and Claude Fable 5.2 is selected.](/blog/fable-for-planning/pm-fable.png "The PM's model list, with Claude Fable 5.2 selected.")

![New agent dialog for a Frontend Engineer. Claude Code is chosen and marked Ready, and the model is Claude Opus 5.5.](/blog/fable-for-planning/frontend-opus.png "The Frontend Engineer uses Claude Code, with Claude Opus 5.5 selected.")

## Which one to pick

- **One Claude Code session, and the plan should be on Fable:** `/model fable`, then `/model opus` or `/model sonnet`. Use `opusplan` when Opus and Sonnet are enough.
- **A cheaper model for a step you delegate:** a subagent with `model: opus` or `model: sonnet`.
- **A plan you want in a new session:** paste it into a session on Opus or Sonnet.
- **Many tickets, with the planner on Fable and the coder on Opus or Sonnet:** Coppice. Start with [Getting started](/docs/getting-started).
