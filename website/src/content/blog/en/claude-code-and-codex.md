---
title: How to use Claude Code and Codex together on one project
metaTitle: Claude Code and Codex on one project
description: "Create one Claude Code agent and one Codex agent. Configuring both does not make both run. You assign or confirm the assignee for each stage."
answer: "Create one agent on Claude Code and another on Codex. Configuring both connectors does not make both run. A ticket has one assignee. You assign or confirm that assignee for each stage, and the Technical Lead's assignment decides who implements. You approve the plan unless you skip it, then accept the diff before a separate merge."
published: "2026-11-07"
draft: true
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - agents
  - tickets
---

Coppice is a desktop app. It runs the `claude` and `codex` commands already on your computer. Each agent has one connector, and a ticket stores one assignee. Configuring Claude Code on one agent and Codex on another does not make both run. You assign or confirm the assignee for each stage. The Technical Lead's assignment decides which agent implements the work.

Install and sign-in for the app and the CLIs are in the docs. This note covers how the two agents share one project, and where you still have to act.

## Before you create the agents

Install the [desktop app](/docs/install). You also need a local git checkout of the project.

Install both CLIs, then sign in to each one in its own terminal, the way that CLI asks you to. Do not paste a token into Coppice. Coppice does not take a provider key on the agent form. It checks the login that is already on the machine: `claude auth status` for Claude Code, and `codex login status` for Codex.

Open **System**, then **Connectors**.

- **Ready** means the local check found a sign-in.
- **Found, not signed in** means the command is there and the check says you are signed out. Sign in in that CLI's terminal, then click **Run check**.
- **Not on your PATH** means Coppice cannot find the command. Install it, then click **Run check**.

The install links and the full connector list are on the [Providers](/docs/providers) page. Adding the checkout and the board is in [Getting started](/docs/getting-started): **Repositories** → **Add repository**, then a board from **Boards**. The repository row should show **Ready** before you run a ticket.

> **TODO capture:** System → Connectors, with Claude Code and Codex both **Ready**. Demo data only. No token, email, home path, or real repository name. Alt: "Connectors list with Claude Code and Codex marked Ready." Caption: "Demo: both CLIs pass the local sign-in check."

## Pick the role, the connector, and the model

Open **Agents** and click **New agent**.

1. Pick a **Preset**. The menu shows the preset key and the role, such as `pm — PM`, `tech_lead — Technical Lead`, or `backend_engineer — Backend Engineer`. The preset fills **Role**. On this form, **Role** is read-only.
2. Type a **Name**.
3. Pick a **Connector**. Choose **Claude Code** or **Codex**. The row includes the status, such as **Claude Code — Ready**.
4. Under **Model**, leave the connector's default, or pick the model for that role. The first option is named for the connector, such as **Claude Code's default**. If you installed a model after opening the form, click **Refresh models**.
5. Click **Create agent**.

Claude Code also shows **Thinking level**. Codex does not. Leave the level as it is, or pick one. Still pick the model for each role.

If that save turns the connector on, Coppice shows "Claude Code turned on." or "Codex turned on."

You need three roles for the handoff below: a **PM**, a **Technical Lead**, and one implementer (**Backend Engineer**, **Frontend Engineer**, or **Research**). Put Claude Code on one of them and Codex on another. The third role uses either connector. One way to split them is a PM on Claude Code, a Backend Engineer on Codex, and a Technical Lead on either CLI.

> **TODO capture:** New agent form with Preset, read-only Role, Connector on Claude Code or Codex, and Model. Demo data. No other product names, tokens, or home paths. Alt: "New agent form with a role, Claude Code or Codex, and a model." Caption: "Demo: one agent, one connector. Pick the model for this role."

## A ticket has one assignee

**Assignee** on the ticket is one agent. You assign or confirm that name for each stage. The Technical Lead's assignment decides which agent implements. With the default auto-assign for **Ready**, Coppice sets **Assignee** to the agent the Technical Lead named. If that is not the agent you want, change **Assignee**.

The plan is written by the earliest enabled PM. If you create one PM and that PM is the assignee, that agent writes the plan. An implementer runs after the Technical Lead names them and you confirm that assignee.

[Concepts](/docs/concepts) describes the columns. The steps below are the controls you use.

## Write the ticket and start it

1. On the board, in **Backlog**, type a title in **Add ticket…** and press Enter. Open the card.
2. Set **Assignee** to your PM. Set **Repository** to the checkout that shows **Ready**.
3. Leave **Skip planning** unchecked when you want a plan. The checkbox sits under **Status** and saves as soon as you change it. Checking it does not move the ticket. With it on, Coppice allows a move to **Ready** or **In Progress** without an approved plan, and **Run Agent** does not start the PM's planning run.
4. Click **Save metadata**. You should see "Metadata saved". On a new ticket this writes the assignee before the repository, so this save does not start a run.
5. Click **Run Agent** in the ticket header. It stays off until the ticket has an assignee and a repository that is **Ready**. After it starts, the ticket opens **Live Console**. **Stop** ends the run.

The Backlog card can show **Plan needs a run** and a **Run agent** button. That card button starts a planning run. It is a separate control from the header **Run Agent**.

Header **Run Agent** on a Backlog ticket starts planning when the assignee is a PM and **Skip planning** is off. The planner is the earliest enabled PM. With one PM, that is the agent you assigned. The ticket moves to **Plan Review** when the PM posts questions or a plan. If that plan run edits files, Coppice discards the edits and posts "The plan run changed some files. Coppice discarded them, and your ticket's code is unchanged."

If the card says **Waiting for your answers**, the PM asked before writing the plan. Answer on the ticket and click **Send answers**. **Approve plan** is not offered while those questions are waiting.

When a plan is there, it is the **Proposed plan**, with **Approve plan** and **Ask for changes** under it. Approving adds the plan to the description under **Approved plan**, moves the ticket to **Ready**, assigns the earliest enabled Technical Lead, and starts that agent's run. You can also click **Ask for changes** and say what should change.

> **TODO capture:** Ticket drawer for a demo ticket: one Assignee, Repository named Demo, **Skip planning** visible, and **Run Agent**. No marketing caption, email, home path, or real repository name. Alt: "Ticket with one assignee, a demo repository, Skip planning, and Run Agent." Caption: "Demo: one assignee. Run Agent starts the PM when Skip planning is off."

## What continues on its own

`auto_start_runs` defaults to true. After a run hands the ticket to another agent, Coppice sets **Assignee** to that agent and starts the run. The setting is in [Settings](/docs/settings). Turn it off when you want to click **Run Agent** for each later stage.

That handoff still needs a person at two points:

- **Plan Review**, unless **Skip planning** is on. Approving the plan is what assigns the Technical Lead and starts that run.
- **Wait for Human Review**, where you accept the diff. Merge is a separate step after **Done**.

If there is no Technical Lead, the ticket stays in **Ready** with no assignee. The comment says: "No Technical Lead agent is available, so this ticket stays in Ready without an assignee. Assign an agent to start work."

If the Technical Lead does not name an implementer, the ticket stays in **Ready** and the implementation run does not start. The comment on the ticket says why. With the default auto-assign for **Ready**, a named implementer becomes the only assignee, and that agent's run starts. Confirm that name. To use the other connector, set **Assignee** to that agent.

A stuck ticket can move to **Blocked** with a comment. Read the comment before you start it again.

## Review the diff, then accept, then merge

When the ticket is in **Wait for Human Review**, open it and click **Review code** to read the diff. Click **Accept**. The dialog title is **Accept this ticket?** Confirm with **Accept**. The ticket moves to **Done**.

Accepting does not merge. A Done card can show **Accepted, not merged** and a **Merge…** button. **Merge…** is a separate step. **Create PR via API** is the other choice on that card. **Merge…** merges the commit you accepted in Human Review.

If that merge conflicts and Coppice can abort it cleanly, the message names the files and ends with "Nothing was changed." The ticket stays in **Done**. The button is **Ask {agent name} to resolve**, with that agent's name filled in. Choosing it moves the ticket to **In Progress** with that agent. The comment says you accept it again before it merges.

> **TODO capture:** Done card on a demo board, showing **Accepted, not merged** and **Merge…** apart from **Accept**. No marketing caption, email, home path, or real repository name. Alt: "Done card marked Accepted, not merged, with a separate Merge button." Caption: "Demo: Accept and Merge are separate. Not a Claude Code or Codex result."

## When a run does not start

- **Run Agent** is off, with "Assign an agent before running." Set **Assignee**.
- **Run Agent** is off, with "Link a repository before running." Set **Repository** and save.
- **Run Agent** is off, with "Repository path is not ready. Ask an admin to verify in Settings → Repositories." The repository row is not **Ready**.
- **Run Agent** is off, with "An agent run is already queued or running." Assigning an agent on a ticket that already has a repository starts a run when `auto_start_runs` is true. Wait for it, or click **Stop**.
- The run reports "No PM agent is available to write the plan. Add one in Agents, or turn on Skip planning." Add an enabled PM, or check **Skip planning**.
- You moved the ticket toward **In Progress** and Coppice says "Approve the plan, or turn on Skip planning, before moving this ticket to In Progress." Approve the plan first, or check **Skip planning**.
- Claude Code older than 2.0.0 fails when the run starts. Coppice tells you to update it. Codex has no version floor in Coppice.
- A **Review cap** on the board can hold the next run in **Ready**. The hint under **Review cap** is "How many tickets can wait for your review at once. Leave it empty for no cap."

Claude Code chats show "Coppice hasn't confirmed that Claude Code stays read-only. The agent can still run commands on your computer." Codex chats do not show that banner. Ticket work is separate from chat: the ticket run is the one that edits the worktree.

The steps above are the controls. **Live Console** shows the run that starts on your machine.

## Read the setup pages

- [Install](/docs/install) for the desktop app.
- [Providers](/docs/providers) for installing the CLIs and reading connector status.
- [Getting started](/docs/getting-started) for the first repository, board, and ticket.
- [Concepts](/docs/concepts) for how a ticket moves, including **Skip planning** and automatic continuation.
- [Settings](/docs/settings) for `auto_start_runs` and the other workflow switches.
