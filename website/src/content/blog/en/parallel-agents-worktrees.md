---
title: How to run multiple AI coding agents in parallel using Git worktrees
metaTitle: Parallel agents with Git worktrees
description: "Give each independent task its own ticket, branch, and worktree. Review the diffs, then merge in order. Separate checkouts can still conflict."
answer: "Give each independent task its own ticket. Coppice creates a branch and a separate Git checkout for that ticket, and you start the work with Run Agent. Review each diff, then merge in dependency order. Separate checkouts keep agents out of the same files, but they do not prevent conflicts when the branches merge."
published: "2026-11-07"
author: Coppice
authorType: Organization
draft: true
keywords:
  - git worktree
  - parallel agents
  - tickets
  - code review
---

Each ticket has one assignee at a time. When implementation starts, Coppice adds one branch and one extra checkout, and later agents on that ticket share them. Parallel work is two tickets. A role name such as Technical Lead is a label on the agent. The assignee field and the run list show who is working.

A branch is a name for a line of commits. A worktree is another folder that belongs to the same repository, with its own checked-out branch. Coppice creates the folder and the branch. Leave that folder alone: do not add a worktree inside it, and do not commit, rebase, or edit files there yourself.

A card shows a mark labeled **Agent running** while a run is queued or running. Read the diff and the run before you treat that ticket as finished.

## What you need first

Install Coppice, install and sign in to one supported agent CLI, and register a local Git checkout. Those steps stay in the existing guides: [Install](/docs/install), [Getting started](/docs/getting-started), [Providers](/docs/providers), and [Concepts](/docs/concepts).

Use a repository you can throw away for the example below. Before an implementation run creates a ticket branch, leave that registered checkout on the commit both tasks should share, usually the tip of the default branch. The server creates the branch with `git worktree add -b <branch> <path>` and does not pass a start commit. That Git command starts the new branch at the checkout's current commit. On a throwaway repository, two calls of that command produced two branches at the same current commit.

The registered checkout also has to be clean before **Merge…**. If it has uncommitted changes, the merge stops with "main repository has uncommitted changes — commit or stash before merging". Commit or stash those changes in the registered checkout. That instruction is for the checkout you registered, not for a ticket worktree.

The header button is **Run Agent**. The guides call the same control Run agent. It stays disabled until the ticket has an assignee and a repository that shows **Ready**, and until no run is already queued or running. The button title then says "Assign an agent before running.", "Link a repository before running.", "Repository path is not ready. Ask an admin to verify in Settings → Repositories.", or "An agent run is already queued or running."

`workflow.auto_start_runs` defaults to true. With that value, assigning an agent starts a run only when the ticket already has a repository and is not Done. **Save metadata** assigns first and saves a newly chosen repository after that, so the first save that does both does not start a run. A later assign, once the repository is already saved, can start one. A failed start on that assign request is ignored and the ticket still saves. Click **Run Agent** when you want a run. If one is already active, the button stays off.

## Two tasks, two files

This split shows how to divide file ownership. These paths are an example in this note. Coppice does not ship them, and no agent session created them.

| Ticket | Owns | Leaves alone |
| --- | --- | --- |
| Add a greeting note | `docs/greeting.md` | `docs/farewell.md`, `README.md`, lockfiles, generated files |
| Add a farewell note | `docs/farewell.md` | `docs/greeting.md`, `README.md`, lockfiles, generated files |

Write those limits into each description, including how you will check the result. For the greeting note, the check is that `docs/greeting.md` exists and `docs/farewell.md` is unchanged. Swap the files for the other ticket.

Merge the greeting first when the farewell text has to mention it. When the tasks are independent, either ticket can merge first, and the other one still has to be updated onto that result before you merge it. Separate folders do not cover a shared module, a schema change, a lockfile, a generated file, a shared service, or a test port. Those still need one owner, or they need to be done one after another.

## Start both tickets

1. In **Backlog**, add the two tickets. Open each card, write the description, and click **Save**.
2. Leave **Skip planning** unchecked. With it checked, a ticket can leave Backlog for Ready without a plan. The plan stays part of this walkthrough.
3. On **Detail**, set **Assignee** to a PM agent and **Repository** to the checkout. Click **Save metadata**. One ticket holds one assignee. Repeat for the second ticket, with the same repository.
4. Click **Run Agent** on each ticket. The drawer selects **Live Console** after the request succeeds. You can also open **Agent Runs**.

A PM on Backlog starts planning. Planning uses a scratch checkout and does not create the ticket worktree, and it does not change the ticket's code. When a plan is shown, read it and click **Approve plan**. **Ask for changes** sends a note back. Moving the ticket to In Progress without an approved plan is refused: "Approve the plan, or turn on Skip planning, before moving this ticket to In Progress." Moving it to Ready is refused with "Approve the plan before moving this ticket to Ready, or turn on Skip planning."

After you approve, a Technical Lead can check the plan and choose the implementer. With `auto_start_runs` left at true, that later run can start on its own, so the handoff may continue without another click. If no Technical Lead is available, the ticket stays in Ready, unassigned, and a comment says to assign an agent. Assign the implementer you want. Because the repository is already saved, that assign can start the run when `auto_start_runs` is true. **Run Agent** remains available when no run is active.

An implementer assigned on Backlog, with **Skip planning** off, does not get a work run. **Run Agent** then fails with the In Progress message above until a plan is approved. If you have no PM agent, the planning start says "No PM agent is available to write the plan. Add one in Agents, or turn on Skip planning." Adding agents is covered in [Getting started](/docs/getting-started). This note does not build a team.

The ticket worktree appears when the first work run starts, not during planning. The directory is `TICKET-<id prefix>-<repository slug>` under `agent.worktrees_path`, and the branch is `agent/TICKET-<id prefix>`. The id prefix is the first segment of the ticket id. Agents that work this ticket later share that checkout. The path and the concurrency fields are in [Settings](/docs/settings).

## Read the branch and the checkout

Open the ticket and find **Git actions**. **Ticket branch** is the branch name. **Worktree** is the directory once the work run has created it. Before that, the worktree line shows `(removed)`.

**Review code** opens Code review when the repository is Ready. The **Worktree** list shows the folder name, the branch, and the ticket title. **Base** is the branch you compare against. A long branch can be shortened on screen. **Ticket branch** on the ticket is the full name.

Use those screens for the diff. They show the checkout Coppice created. They are not a record that a particular CLI finished the task.

## Merge one ticket, then update the other

**Accept** and **Merge…** are separate. **Accept** is on the ticket header in **Wait for Human Review**. The dialog title is "Accept this ticket?", and the confirm button is **Accept**. The card can then show "Accepted, not merged".

**Merge…** is in **Git actions** for **Wait for Human Review** and **Done**, and on a Done card that still shows "Accepted, not merged". Choose the base branch. The default is the repository's default branch. Coppice checks out that branch in the registered checkout and merges the ticket branch with a merge commit. The commit that merges is the one you accepted. If no acceptance is recorded, the merge does not run, the ticket is sent back to **Wait for Human Review** when it was elsewhere, and the message is "Not merged. Accept again so Coppice knows which commit you reviewed." If the branch tip has moved, the message is "Not merged. This isn't the commit you reviewed."

A successful merge does not move the other ticket's checkout. A worktree created later starts from whatever commit the registered checkout is on at that moment. A worktree that already exists stays where it is.

Update the second ticket only after its run is finished and its worktree is clean. In **Git actions**, click **Rebase…**. The dialog is "Rebase ticket branch". Keep the base as the default branch unless you mean a different one. Coppice rebases inside the ticket worktree. It tries a fetch first. When that fetch succeeds and `origin/<base>` exists, the rebase uses `origin/<base>`, which can omit a merge that exists only on your local branch. With no remote-tracking ref, it uses the local base branch when that branch is in the worktree. A dirty worktree is refused: "worktree has uncommitted changes — commit or stash before rebasing". Wait until the run has finished and the checkout is clean. Do not stash or edit the ticket worktree to force the rebase.

**Rebase…** does not start an agent and does not run the project's tests. Click **Run Agent** when you want the current assignee to work in the updated checkout. That starts another work run in the same worktree.

If you had already accepted the second ticket and the rebase changes the commit, Coppice marks that acceptance stale. From **Done** or **Wait for Human Review** the ticket goes back to **In Review**. Accept the new diff before you merge. A ticket that was not accepted yet keeps its status.

Then open the diff, accept it, and merge the second ticket the same way.

## When the merge conflicts

A conflict during **Merge…** aborts the merge. When that abort leaves the registered checkout clean, Coppice reports that nothing was changed and lists the files. The base branch does not gain the ticket's commits. After you have accepted, the ticket is in **Done**, and it stays there until you ask an agent to resolve the conflict.

The dialog button reads **Ask**, the agent's name, and **to resolve**. On a Done ticket, choosing it moves the ticket to **In Progress**, sets that agent as the assignee, and starts a work run. The comment says you will need to accept again in Human Review before it merges. Accept the new diff, then merge again. A merge conflict before the ticket is Done can start a resolve run and leave the status and assignee as they are.

A rebase conflict aborts the rebase. When the worktree is clean after the abort, the same kind of offer lists the files and says nothing was changed. If the abort leaves the worktree dirty, Coppice reports that the worktree may still be dirty. Stop there. Do not repair that folder by hand.

## If the second run waits

Two tickets can be in progress together only while a worker is free for each run.

Coppice starts `agent.worker_count` workers when the process starts. The default is 8, and a value below 1 runs as 1. Each connector also has `max_concurrent_runs`. The default is 2. Extra jobs stay queued and start oldest-first as a slot frees. The workers read those numbers from the config loaded at startup. Saving `worker_count` reports that a restart is required. Saving `max_concurrent_runs` is treated as a connector change and does not report a restart, but the workers still have the startup copy. This note did not restart a process or watch a queue move. The field descriptions are in [Settings](/docs/settings).

A board **Review cap** can also hold a Ready ticket. The hint is "How many tickets can wait for your review at once. Leave it empty for no cap." An empty cap does not hold the ticket.

**Run Agent** also fails when the assignee's connector is turned off or not configured. Sign in with that CLI, then check **System → Connectors**. Coppice does not take a provider API key for this. [Providers](/docs/providers) has the install and sign-in checks.

## Where to go next

[Concepts](/docs/concepts) is the status list, including **Plan Review** and **Wait for Human Review**. [Getting started](/docs/getting-started) is the first ticket. [Providers](/docs/providers) is the CLI you already sign in to. [Settings](/docs/settings) is `worktrees_path`, `worker_count`, and `max_concurrent_runs`.

Removing old worktrees, and building a larger team, are outside this note. Different agents can use different models. This note does not compare those models or their cost.
