---
layout: ../../layouts/Docs.astro
title: Getting started
description: "Go from a fresh install to your first agent run in five steps."
---

# Getting started

Five steps take you from a fresh install to your first agent run. You need a local git checkout and one agent CLI. No Coppice account.

On a fresh install, Coppice opens a short setup that adds your first repository, board, and starter team. You can reopen it from **System → Connectors** with **Set up workspace**.

## 1. Install a provider and sign in
A provider is the agent CLI Coppice runs for you: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code or Grok. Coppice doesn't install it. It finds the CLIs already on your computer.

1. Install the CLI and sign in to it in your terminal, the way you normally would.
2. In Coppice, open **System** in the sidebar, then **Connectors**.
3. Find your CLI's card. It shows **Ready** when Coppice can use it.

If it shows **Found, not signed in**, sign in to the CLI in your terminal. If it shows **Not on your PATH**, install it, or follow **Install docs** on the card. Then click **Run check**.

Kilo Code, GitHub Copilot, Qwen Code and Kimi Code show **Installed**. Coppice can't check their sign-in without using your plan, so a run shows an error if you're signed out.

## 2. Set up an agent
1. Open **Agents** in the sidebar and click **New agent**.
2. Pick a **Preset**. It fills in the role and prompt.
3. Type a **Name**.
4. Pick a **Connector**. Coppice selects the first one that's ready.
5. Pick a **Model**, or leave it on the default.
6. Click **Create agent**.

Saving turns that connector on. You'll see "{name} turned on."

Add a **PM** agent, a **Technical Lead** agent, and at least one agent to do the work (**Frontend Engineer**, **Backend Engineer**, or **Research**). Planning and handoffs need all three.

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
1. **Plan Review.** The PM agent writes a plan, and the ticket moves to Plan Review. The plan shows right under the description, with **Approve plan** and **Ask for changes** below it. Approving adds the plan to the description under **Approved plan**, after your own text.
2. **Ready.** When you approve, the ticket moves to Ready. Your Technical Lead agent checks the plan against the code, corrects the approved plan only if it needs to, and picks who does the work. That agent starts right away. If nobody fits, the ticket stays unassigned with a comment that says why.
3. **In Progress.** The assigned agent does the work in its own git worktree.
4. **Wait for Human Review.** You read the diff. Click **Accept** in Human Review.
5. **Done.** A Done card may show "Accepted, not merged". Open the ticket and choose **Merge…**, or **Create PR via API** if you'd rather review it as a pull request.

To skip the plan, check **Skip planning** on the ticket. It goes from Backlog straight to Ready.
