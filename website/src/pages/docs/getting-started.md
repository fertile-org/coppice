---
layout: ../../layouts/Docs.astro
title: Getting started
metaTitle: "Getting Started: Your First Coding Agent Run"
description: "Install Coppice, sign in to one agent CLI in your terminal, then run a ticket. Coppice never asks for a provider API key. You don't need a Coppice account."
---

# Getting started

Sign in to one agent CLI before the five steps below. You also need the Coppice app and a local git checkout. You don't need a Coppice account. Coppice does not install the CLI, and it never asks for or stores a provider API key.

## What you need

- Coppice, from the package for your platform and architecture. See [Install](/docs/install).
- A local git checkout Coppice can write to.
- One supported agent CLI on the same computer. Command names and vendor install guides are on [Providers](/docs/providers).

## Sign in first

Sign in to that CLI in your terminal, the way you normally would. The sign-in stays with the CLI. Coppice does not ask you to paste a provider API key.

To use a key with a provider the CLI doesn't offer, connect that provider in OpenCode. OpenCode keeps the key. See [Providers](/docs/providers#use-your-own-api-key-or-another-provider).

On a fresh install, Coppice opens a short setup that adds your first repository, board, and starter team: PM, Technical Lead, Backend Engineer, Frontend Engineer, and QC. You can reopen it from **System → Connectors** with **Set up workspace**.

## 1. Check the connector
A provider is the agent CLI Coppice runs for you: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code or Grok. Coppice finds the CLIs already on your computer.

1. Open **System** in the sidebar, then **Connectors**.
2. Find your CLI's row. It shows **Ready** when a free sign-in check confirmed you are signed in. Open the row for details.

If it shows **Found, not signed in**, sign in to the CLI in your terminal. If it shows **Not on your PATH**, install it, or follow **Install docs** in the details drawer. Then click **Run check**.

Claude Code, Codex, Cursor, OpenCode, and Grok have that free check. It does not open a browser, prompt you, or call a model.

Kilo Code, GitHub Copilot, Qwen Code and Kimi Code show **Installed**. Coppice can't check their sign-in without using your plan, so a run shows an error if you're signed out.

## 2. Set up an agent
1. Open **Agents** in the sidebar and click **New agent**.
2. Pick a **Preset**. It fills in the role and prompt.
3. Type a **Name**.
4. Pick a **Connector**. Coppice selects the first one that's ready.
5. Pick a **Model**, or leave it on the default.
6. Click **Create agent**.

Saving turns that connector on. You'll see "{name} turned on."

Add a **PM** agent, a **Technical Lead** agent, and at least one agent to do the work (**Frontend Engineer**, **Backend Engineer**, or **Research**). Planning and handoffs need all three. Add a **QC** agent too, so tickets that pass code review get verified in In QA.

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="The new agent form with a name, a role and a CLI chosen.">
<figcaption>Each agent runs on one of your CLIs.</figcaption>
</figure>

## 3. Add a repository
1. Open **Repositories** in the sidebar and click **Add repository**.
2. Next to **Local path**, click **Browse…** and pick a local git checkout.
3. Coppice fills in **Name**, **Remote URL (optional)** and **Default branch** from the checkout when it can. Check them.
4. Click **Add repository**.

The new row should show **Ready**. If it shows **Path missing** or **Not a git repo**, pick the folder again.

## 4. Set up a board
1. Open **Boards** in the sidebar.
2. Click **New board**. On a fresh install, click **Create board**.
3. Type a **Name** and click **Create board**.

Coppice opens your new board.

## 5. Create a ticket and run it
1. In the **Backlog** column, type a title in **Add ticket…** and press Enter.
2. Open the card.
3. Click **Edit** to add a description, then **Save**.
4. On **Detail**, set **Assignee** to your agent and **Repository** to your repository. Click **Save metadata**.
5. Click **Run agent**.

The ticket switches to **Live Console** so you can watch the agent work. Click **Stop** to end the run.

**Run agent** stays off until the ticket has an assignee and a repository that shows **Ready**. Assigning an agent doesn't start a run by itself.

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="A board with a new ticket in Backlog and the Run agent button showing.">
<figcaption>Press Run agent, and the PM writes the plan.</figcaption>
</figure>

## What happens next
You approve the plan before coding starts, unless you choose Skip planning. Once you start a ticket, agents can continue automatically through the workflow. You review and accept the final diff. Accept and Merge are separate choices.

1. **Plan Review.** The PM agent writes a plan, and the ticket moves to Plan Review. The plan shows right under the description, with **Approve plan** and **Ask for changes** below it. Approving adds the plan to the description under **Approved plan**, after your own text.
2. **Ready.** When you approve, the ticket moves to Ready. Your Technical Lead agent checks the plan against the code, corrects the approved plan only if it needs to, and picks who does the work. That agent's run starts on its own. To start runs yourself, set `auto_start_runs = false` in Settings. If nobody fits, the ticket stays unassigned, with a comment that says why.
3. **In Progress.** The assigned agent does the work in its own git worktree.
4. **Wait for Human Review.** You read the diff. Click **Accept** in Human Review.
5. **Done.** A Done card may show "Accepted, not merged". Open the ticket and choose **Merge…**, or choose **Create PR via API** if you'd rather review it as a pull request. If you merge that pull request on GitHub.com, Coppice records the merge on the next sweep (at startup and every 5 minutes) and the label clears. Repositories that are not github.com, or that have no forge token, keep the label until you merge in Coppice.

To skip the plan, check **Skip planning** on the ticket. It goes from Backlog straight to Ready.
