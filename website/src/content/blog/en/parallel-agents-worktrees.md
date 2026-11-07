---
title: How to run multiple AI coding agents in parallel using Git worktrees
metaTitle: Parallel agents with Git worktrees
description: "Give each independent task its own ticket, branch, and worktree. Review each diff, merge in order, then check the combination."
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

Each ticket has one assignee. When implementation starts, Coppice adds one branch and one extra checkout, and later agents on that ticket share them. Parallel work is two tickets. A role such as Technical Lead is a label on the agent. The assignee field and the run list show who is working.

A branch is a name for a line of commits. A worktree is another folder of the same repository, with its own checked-out branch. Coppice creates the folder and the branch. Leave that folder alone: do not add a worktree inside it, and do not commit, rebase, or edit files there yourself.

The steps below name the controls in the current app. This note did not run them as a Coppice session. A card can show a mark labeled **Agent running** while a run is queued or running. Read the diff and the run before you treat that ticket as finished.

## What you need first

Install Coppice, sign in to one supported agent CLI, and register a local Git checkout. Those steps stay in the existing guides: [Install](/docs/install), [Getting started](/docs/getting-started), [Providers](/docs/providers), and [Concepts](/docs/concepts).

Use a repository you can throw away. The paths in the next section are example names for that repository. Coppice does not include those files, and no agent created them for this note.

Before an implementation run creates a ticket branch, leave the registered checkout on the commit both tasks should share, usually the tip of the default branch, and leave it clean. Coppice creates the branch with `git worktree add -b <branch> <path>` and does not name a start commit, so the new branch starts at that checkout's current commit. Outside Coppice, on a throwaway repository, two calls of that command produced two branches at the same current commit.

**Merge…** also needs the registered checkout clean. Commit or stash any uncommitted work there, in the checkout you registered, before you merge. That is not a change inside a ticket worktree.

The header button is **Run Agent**. The guides call the same control Run agent. It stays off until the ticket has an assignee and a repository that shows **Ready**, and until no run is already queued or running.

`workflow.auto_start_runs` defaults to true. Assigning an agent then starts a run only when the ticket already has a repository and is not Done. **Save metadata** assigns first and saves a newly chosen repository after that, so the first save that sets both does not start a run. A later assign, once the repository is already saved, can start one. Click **Run Agent** when you want a run.

## Choose a split you can merge

A safe split gives each ticket files the other must not change, and it keeps shared files out of both tickets. Lockfiles, generated files, schemas, and shared modules need one owner, or they belong in a later ticket. A shared service or a test port also needs one owner: two worktrees are not two copies of a running server.

| Ticket | Owns this example file | Leaves alone |
| --- | --- | --- |
| Add a greeting note | `docs/greeting.md` | `docs/farewell.md`, `README.md`, lockfiles, generated files |
| Add a farewell note | `docs/farewell.md` | `docs/greeting.md`, `README.md`, lockfiles, generated files |

Write the owned path and the check into each description. For the greeting ticket, the check is that `docs/greeting.md` exists and `docs/farewell.md` is unchanged. Swap those paths for the farewell ticket.

Two conflicts can still show up at merge time.

**Overlapping files.** Both tickets change one path, or both regenerate a lockfile. Separate folders do not stop that. The merge or rebase stops and lists the files.

**A semantic conflict.** The paths differ, and the combination is still wrong. `docs/farewell.md` can tell the reader to run the command that `docs/greeting.md` just renamed. Each ticket can pass its own check. Git can merge the two files cleanly. You see the problem only when you read them together.

Merge the greeting first when the farewell text depends on it. When the tasks are independent, still pick an order. Merge one ticket, update the other onto that result, review the combination, and only then merge the second. Merging both straight into the default branch skips that check.

## Start both tickets

1. In **Backlog**, add the two tickets. Open each card, write the description, and click **Save**.
2. Leave **Skip planning** unchecked. With it checked, a ticket can go from Backlog to Ready without a plan. This walkthrough keeps the plan.
3. On **Detail**, set **Assignee** to a PM agent and **Repository** to the checkout. Click **Save metadata**. One ticket holds one assignee. Do the same for the second ticket and the same repository.
4. Click **Run Agent** on each ticket. The drawer selects **Live Console** after the request succeeds. **Agent Runs** lists the runs.

A PM on Backlog writes the plan in a scratch checkout. That run does not create the ticket worktree, and it does not change the ticket's code. Read the plan and click **Approve plan**. **Ask for changes** sends a note back. Approve the plan before the ticket moves on, unless you turn **Skip planning** on.

After you approve, a Technical Lead can choose the implementer. With `auto_start_runs` left at true, that later run can start without another click. If no Technical Lead is available, the ticket stays in Ready and a comment tells you to assign an agent. Assign the implementer you want. The repository is already saved, so that assign can start the run. **Run Agent** remains available when no run is active. Adding agents is covered in [Getting started](/docs/getting-started).

The ticket worktree appears when the first work run starts. The directory is `TICKET-<id prefix>-<repository slug>` under `agent.worktrees_path`, and the branch is `agent/TICKET-<id prefix>`. The id prefix is the first segment of the ticket id. Later agents on this ticket share that checkout. The path and the concurrency fields are in [Settings](/docs/settings).

## Read each checkout

Open the ticket and find **Git actions**. **Ticket branch** is the branch name. **Worktree** is the directory once the work run has created it. Before that, the worktree line shows `(removed)`.

**Review code** opens Code review when the repository is Ready. The **Worktree** list shows the folder name, the branch, and the ticket title. **Base** is the branch you compare against. A long branch can be shortened on screen. **Ticket branch** on the ticket is the full name.

Use those screens for the diff. They show the checkout Coppice created. They are not a record that a particular CLI finished the task.

## Accept, merge, then update the other ticket

**Accept** and **Merge…** are separate actions. **Accept** is on the ticket header in **Wait for Human Review**. The dialog title is "Accept this ticket?", and the confirm button is **Accept**. The card can then show "Accepted, not merged". Merge only the commit you accepted.

**Merge…** checks out the base branch in the registered checkout and merges the ticket branch. The default base is the repository's default branch. A successful merge does not move the other ticket. A worktree that already exists stays on its old commits. A worktree created after the merge starts from whatever commit the registered checkout is on at that moment.

You update the second ticket yourself, after its run has finished and its worktree is clean. In **Git actions**, click **Rebase…**. Keep the default branch unless you mean a different base. Coppice rebases inside the ticket worktree. If a fetch succeeds and `origin/<base>` exists, the rebase uses that remote-tracking ref. A merge that exists only on your local branch can be absent from it. With no remote-tracking ref, the rebase uses the local base branch.

**Rebase…** does not start an agent, and it does not run the project's tests. The retest is a second human action. Click **Run Agent** when you want the current assignee to work in the updated checkout. That starts another work run in the same worktree. Then read both example files together and apply the check you wrote on the ticket.

If you had already accepted the second ticket and the rebase changes the commit, Coppice marks that acceptance stale. From **Done** or **Wait for Human Review** the ticket returns to **In Review**. Accept the new diff, then use **Merge…**.

## When the two results disagree

An overlapping-file conflict during **Merge…** aborts the merge. When the registered checkout is clean after that abort, the base branch does not gain the ticket's commits, and the dialog lists the files. After you have accepted, the ticket stays in **Done** until you ask an agent to resolve it. The button reads **Ask**, the agent's name, and **to resolve**. On a Done ticket, that choice moves the ticket to **In Progress**, sets that agent as the assignee, and starts a work run. Accept the new diff before you merge again.

A rebase conflict aborts the rebase and can offer the same kind of help. If the worktree is left dirty, stop. Do not repair that folder by hand.

A semantic conflict has no file list. **Rebase…** can succeed while `docs/greeting.md` and `docs/farewell.md` contradict each other. Read the combined checkout, and run the second ticket's check, before you accept it.

## If the second run waits

Two tickets proceed together only while a worker is free for each run. At startup Coppice uses `agent.worker_count` workers, default 8, and each connector's `max_concurrent_runs`, default 2. Extra jobs stay queued. Those are the numbers loaded when the process starts. The fields are in [Settings](/docs/settings).

A board **Review cap** can hold a Ready ticket when you set one. Leave the field empty for no cap.

**Run Agent** also fails when the assignee's connector is turned off. Sign in with that CLI, then check **System → Connectors**. Coppice does not take a provider API key for this. [Providers](/docs/providers) has the sign-in checks.

## Where to go next

[Concepts](/docs/concepts) is the status list, including **Plan Review** and **Wait for Human Review**. [Getting started](/docs/getting-started) is the first ticket. [Providers](/docs/providers) is the CLI you sign in to. [Settings](/docs/settings) is `worktrees_path`, `worker_count`, and `max_concurrent_runs`.

Removing old worktrees, and building a larger team, are outside this note. Different agents can use different models. This note does not compare those models or their cost.
