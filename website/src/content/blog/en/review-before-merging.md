---
title: How to review AI-generated code before merging
description: "Approve the plan in Plan Review, or turn on Skip planning and move the ticket yourself. Submit diff comments, accept the commit, and merge only when you choose."
answer: "You approve the plan in Plan Review, or you turn on Skip planning and then move the ticket to Ready. The checkbox leaves the column unchanged. Later runs can start on their own. Accept records the commit you reviewed. Merge… is a separate step, and it merges that commit."
published: "2026-11-07"
author: Coppice
authorType: Organization
draft: true
keywords:
  - plan review
  - code review
  - merge
---

Hire specialists. Keep the final say. In Coppice that say is two reviews, then a merge you start yourself. You approve the plan in **Plan Review**, or you skip planning and move the ticket. Agents can then build, review, and test without another click from you. In **Wait for Human Review** you read the diff and choose **Accept**. **Accept** records that commit and moves the ticket to **Done**. **Merge…** is the later decision.

## What you need first

You need the desktop app, a local git checkout, and one agent CLI you have already signed in to. Add a PM, a Technical Lead, and one agent who does the work, such as a Frontend Engineer or a Backend Engineer. Add a QC agent as well when you want a ticket that passes review to be checked in **In QA**.

[Getting started](/docs/getting-started) walks through the install, the first agent, the repository, and the board. [Concepts](/docs/concepts) names every column. [Settings](/docs/settings) lists `auto_start_runs`. It defaults to `true`.

Click **Run Agent** in the ticket header to start the first run. A Backlog card that still needs a plan shows **Run agent**. Assigning an agent does not replace that click. **Save metadata** can assign the agent before it saves a repository you just picked, and a run starts from an assignment only when the ticket already has a repository.

## Approve the plan

1. Create the ticket in **Backlog**, set the repository, and assign your PM.
2. Click **Run Agent**.
3. Read the plan when the ticket reaches **Plan Review**. It is labeled **Proposed plan**.
4. Click **Approve plan**, or click **Ask for changes** and write what should change.

Approving writes the plan into the description under **Approved plan**, after your own text, and posts “Plan approved.” The ticket moves to **Ready**. The Technical Lead checks that section against the code, corrects it only when it needs a correction, and picks who does the work. That run starts then.

Planning does not change the ticket's code. If the plan run did change files, Coppice discards them and posts that your ticket's code is unchanged.

If the ticket is unclear, the PM may ask 1 to 3 short questions before writing the plan. The card shows “Waiting for your answers”. Answer in the ticket and click **Send answers**. You can approve a plan only while the ticket is in **Plan Review**, and only after those questions are answered.

## Skip planning leaves the column where it is

**Skip planning** is the exception for a ticket that should not wait for a plan. The checkbox sits under **Status**. Checking it saves immediately. The ticket stays in its current column. Checking the box does not move it to **Ready**.

Move it yourself. Set **Status** to **Ready** and click **Save metadata**, or drag the card onto **Ready**. The board says “Drag tickets between columns to update status.” Entering **Ready** is what starts the Technical Lead, when one is available and the repository is ready.

Without an approved plan, and with **Skip planning** off, those moves are refused:

- “Approve the plan, or turn on Skip planning, before moving this ticket to In Progress.”
- “Approve the plan before moving this ticket to Ready, or turn on Skip planning.”

If no Technical Lead is available, the ticket stays in **Ready**, unassigned, with this comment: “No Technical Lead agent is available, so this ticket stays in Ready without an assignee. Assign an agent to start work.”

If the checkbox change fails, Coppice shows “Couldn't change Skip planning. Try again.”

## Later runs can start on their own

Leave `auto_start_runs` at `true` unless you want to start every later run yourself. After the Technical Lead finishes, the implementer starts, then the review, then QA. You do not click before each of those runs.

On the usual path the ticket moves like this:

1. **In Progress.** The assigned agent works on the ticket's own branch. Your main checkout stays untouched until you merge.
2. **In Review.** A reviewer, or the Technical Lead, reviews that work. When that run finishes, the ticket moves to **In QA**.
3. **In QA.** The QC agent checks it. When that run finishes, the ticket moves to **Wait for Human Review**.

Set `auto_start_runs` to `false` in **Settings** when those later runs should wait for **Run Agent**. The same setting is what starts a run when you assign an agent to a ticket that already has a repository.

A missing reviewer or QC agent leaves the ticket unassigned in that column and posts a comment that names the missing role. The column does not turn into **Blocked** by itself. A ticket that is actually stuck goes to **Blocked** with a comment that says why.

One limit on that path: after a clarification round, an implementer who finishes from **In Progress** can land in **Wait for Human Review** without a stop in **In QA**. The usual path above still includes **In Review** and **In QA**.

A full **Review cap** can hold a ticket in **Ready**, or keep it from entering **Wait for Human Review**, and a comment explains the hold. An empty cap means no waiting limit. Review that does not pass can stop with “Review didn't pass after [n] rounds. This may need a different approach.”

## Submit line comments before they leave the diff

Open the ticket and click **Review code**. The page says **Code review**. **Review code** stays off until the ticket has a repository that shows **Ready**.

Click a line number. The field says “Leave a comment…”. **Add comment** keeps the note on the diff in this page. It does not post the note to the ticket, and it does not move the card. The note then shows **Edit** and **Remove**. **Edit** offers **Save**.

Click **Submit review** when the note should become a review. The dialog is **Submit review**. On a ticket it reads “Post a combined review comment on the linked ticket”. **Summary** is required. The placeholder is “Overall feedback for this change…”. If you submit with an empty summary, the dialog says “Review summary is required.”

**Workflow action** has three choices:

- **Comment only** is the default. Coppice posts one comment on the ticket and shows “Review posted”. The column stays where it is, and no run starts.
- **Move to In Progress** moves the ticket to **In Progress** and assigns an engineer. When `auto_start_runs` is `true`, that engineer's run starts. If no engineer is selected, the review is refused with “Pick an engineer before moving this ticket to In Progress.”
- **Reassign engineer** changes the assignee. A run starts from that choice only when the ticket is already in **Backlog**, **Ready**, or **In Progress**.

The posted comment includes your summary and, when you added line notes, an **Inline comments** section. The line, side, and text are listed under the file path.

While the ticket is in **Wait for Human Review** and it has an approved plan, **Code review** shows **Approved plan** beside the diff. Criteria, when the plan has them, read **Met**, **Not met**, or **Not checked**. The step boxes are for reading. A changed file the plan did not name can be marked **Not in the plan**.

## Accept records the commit

**Accept** appears on the ticket only in **Wait for Human Review**. It opens **Accept this ticket?** The dialog says “This accepts the work currently in Wait for Human Review.” **Cancel** is focused. Choose **Accept**.

Coppice records the current tip of the ticket branch, moves the ticket to **Done**, and posts “Accepted: ticket moved to **Done**.” A normal drag cannot put a ticket in **Done**. The refusal is “A ticket moves to Done only when you accept it.”

If an agent adds a commit after that, the ticket goes back to **In Review**. You accept the new tip before it can merge. The shortcut **Accept…** is `Mod+Shift+Enter`. On Linux that is Ctrl+Shift+Enter. The full list is in [Shortcuts](/docs/shortcuts).

## Merge is a separate decision

**Done** means you accepted the work. It does not mean the branch is on your main checkout.

A **Done** card with a current acceptance shows “Accepted, not merged” and **Merge…**. **Merge…** opens **Merge ticket branch**. Pick the **Base branch** and choose **Merge**. The dialog says you can run this more than once. Coppice merges the commit you accepted.

If the branch tip is a different commit, the merge stops with “Not merged. This isn't the commit you reviewed.” If Coppice has no acceptance to compare, it stops with “Not merged. Accept again so Coppice knows which commit you reviewed.”

If files conflict, Coppice leaves the base branch untouched and names the files: “Couldn't merge into [branch] because these files conflict: [files]. Nothing was changed.” The ticket stays in **Done** until you choose **Ask [agent] to resolve**. That sends the ticket back to **In Progress** with that agent. When the conflicts are resolved, the ticket goes back to **In Review**, and you accept it again before it merges. [FAQ](/docs/faq) states the same gate.

**Create PR via API** is the other review path. It needs a GitHub `remote_url` and a forge token. If you merge that pull request on GitHub.com, by merge commit, squash, or rebase, a sweep at startup and every 5 minutes records it and clears “Accepted, not merged”. A repository that is not github.com, or that has no forge token, keeps the label until you merge in Coppice.

**Merge…** on the keyboard is `R then M`, once the button is already shown.

## When a step stops

These are messages this path returns, and a next step that matches each one:

| What you see | What to do |
| --- | --- |
| “No PM agent is available to write the plan. Add one in Agents, or turn on Skip planning.” | Add a PM, or skip planning and move the ticket yourself. |
| “The plan is still being written. Ask for changes once it's done.” | Wait until the plan run finishes. |
| “There's no plan to approve yet.” | The plan has not been posted. Start the PM run again if it never started. |
| “The ticket changed after this plan was written. Ask for changes to get a new plan.” | Click **Ask for changes**. |
| “The plan is approved, but work couldn't start. Check that the ticket has an assignee and its repository is ready.” | Open the repository row. It should say **Ready**. Then assign an agent. |
| “Work couldn't start. Check that the ticket has an assignee and its repository is ready.” | Same check, for a ticket that skipped planning. |
| “This ticket waits in Ready until you have room to review it.” | A ticket is already waiting on you. Accept or raise the **Review cap**. The rest of the sentence says how many are waiting. |
| “This ticket waits here until you have room to review it.” | **Wait for Human Review** is at its cap. The ticket stays in its current column. |
| “main repository has uncommitted changes — commit or stash before merging” | Clean the checkout Coppice merges into, then choose **Merge** again. |
| “Repository has no remote_url. Set one in Settings → Repositories.” | Add the remote before **Create PR via API**. |
| “Repository has no forge token. Set one in Settings → Repositories.” | Add the token in repository settings. |
| “Create PR via API requires a GitHub remote_url.” | The API path talks to GitHub. Merge locally with **Merge…**, or point the remote at github.com. |

Agents run with the permissions of the CLIs on your machine.

## Screenshots

This draft has no screenshots. Figures for **Plan Review**, a review after **Submit review**, and the separate **Accept** and **Merge…** controls are still to come. They need a capture that has passed a safety review. No capture was run for this note, and none is claimed here.

## Read next

Follow [Getting started](/docs/getting-started) for the first ticket. Use [Concepts](/docs/concepts) when you want the whole column sequence, and [Shortcuts](/docs/shortcuts) for **Accept…** and **Merge…**.
