---
layout: ../../layouts/Docs.astro
title: Getting started
description: "Go from a fresh install to your first agent run in five steps."
---

# Getting started

Five steps take you from a fresh install to your first agent run. You need a local git checkout and one agent CLI. No Coppice account.

## 1. Install a provider and sign in
A provider is the agent CLI Coppice runs for you: Claude Code, Codex, Cursor, OpenCode or Kilo Code. Coppice doesn't install it. It finds the CLIs already on your computer.

1. Install the CLI and sign in to it in your terminal, the way you normally would.
2. In Coppice, open **Tools** in the sidebar, then the **Connectors** tab.
3. Find your CLI's card. It shows **Ready** when Coppice can use it.

If it shows **Found, not signed in**, sign in to the CLI in your terminal. If it shows **Not on your PATH**, install it, or follow **Install docs** on the card. Then click **Run check**.

Kilo Code shows **Installed**. Coppice can't check that sign-in without using your plan, so a run shows an error if you're signed out.

[screenshot: step 1]

## 2. Set up an agent
1. Open **Agents** in the sidebar and click **New agent**.
2. Pick a **Preset**. It fills in the role and prompt.
3. Type a **Name**.
4. Pick a **Connector**. Coppice selects the first one that's ready.
5. Pick a **Model**, or leave it on the default.
6. Click **Create agent**.

Saving turns that connector on. You'll see "{name} turned on."

<!-- QUESTION: Should Getting started tell users to create several agents (a PM, a Tech Lead, an implementer) so the "what happens next" flow works? The inventory doesn't say which presets exist. -->

[screenshot: step 2]

## 3. Add a repository
1. Open **Repositories** in the sidebar and click **Add repository**.
2. Next to **Local path**, click **Browse…** and pick a local git checkout.
3. Coppice fills in **Name**, **Remote URL (optional)** and **Default branch** from the checkout when it can. Check them.
4. Click **Add repository**.

The new row should show **Ready**. If it shows **Path missing** or **Not a git repo**, pick the folder again.

[screenshot: step 3]

## 4. Set up a board
1. Open **Boards** in the sidebar.
2. Click **New board**. On a fresh install, click **Create board**.
3. Type a **Name** and click **Create board**.

Coppice opens your new board.

[screenshot: step 4]

## 5. Create a ticket and run it
1. In the **Backlog** column, type a title in **Add ticket…** and press Enter.
2. Open the card.
3. Click **Edit** to add a description, then **Save**.
4. On **Detail**, set **Assignee** to your agent and **Repository** to your repository. Click **Save metadata**.
5. Click **Run Agent**.

The ticket switches to **Live Console** so you can watch the agent work. Click **Stop** to end the run.

**Run Agent** stays off until the ticket has an assignee and a repository that shows **Ready**. Assigning an agent doesn't start a run by itself.

[screenshot: step 5]

## What happens next
1. **Plan Review.** The PM agent writes a plan, and the ticket moves to Plan Review. It waits there for you to approve the plan.
2. **Ready.** When you approve, the ticket moves to Ready. Agents decide who works it next. If nobody fits, they leave it unassigned and add a comment that says why.
3. **In Progress.** The assigned agent does the work in its own git worktree.
4. **Wait for Human Review.** You read the diff. Click **Accept** in Human Review.
5. **Done.** A Done card may show "Accepted, not merged".

To skip the plan, check **Skip planning** on the ticket. It goes from Backlog straight to Ready.

<!-- QUESTION: What is the UI label for approving a plan in Plan Review? Not in the inventory. -->
<!-- QUESTION: When does an accepted ticket merge, and what should users do when a card shows "Accepted, not merged"? -->
