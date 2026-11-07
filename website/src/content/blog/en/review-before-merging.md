---
title: How to review AI-generated code before merging
description: "Before you accept agent code, compare the diff with the approved plan, including unexpected files, error paths, and the tests you can see. Merge comes later."
answer: "Before Accept, compare the diff with the approved plan: unexpected files, error paths, permissions, dependencies, and the test results you can see. Accept records that commit. Merge… is separate. Skip planning leaves the column unchanged until you move the ticket. Later runs can start on their own."
published: "2026-11-07"
author: Coppice
authorType: Organization
draft: true
keywords:
  - plan review
  - code review
  - merge
---

Hire specialists. Keep the final say. You approve the plan, or you skip planning and move the ticket yourself. Agents can then build, review, and test without another click from you. When the card reaches **Wait for Human Review**, the work is ready for you to judge. **Accept** records the commit you reviewed. **Merge…** puts that commit on your base branch later.

## What you need first

You need the desktop app, a local git checkout, and one agent CLI you have already signed in to. Add a PM, a Technical Lead, and one agent who does the work. Add a QC agent when you want a reviewed ticket checked in **In QA**.

[Getting started](/docs/getting-started) is the install path. [Concepts](/docs/concepts) names the columns. `auto_start_runs` defaults to `true` in [Settings](/docs/settings).

Click **Run Agent** in the ticket header to start the first run. A Backlog card that still needs a plan shows **Run agent**. Assigning an agent does not replace that click. A run starts from an assignment only when the ticket already has a repository, and **Save metadata** assigns before it saves a repository you just picked.

## Approve the plan, or skip it and move the ticket

In **Plan Review**, read **Proposed plan**, then click **Approve plan** or **Ask for changes**. Approving writes the plan into the description under **Approved plan**, posts “Plan approved.”, moves the ticket to **Ready**, and starts the Technical Lead. Planning does not change the ticket's code. If the plan run changed files, Coppice discards them.

![Plan Review for the seeded ticket “Review the greeting change.” The proposed plan says to add docs/greeting.md. Approve plan is visible and has not been pressed. Skip planning is unchecked.](/blog/review-before-merging/plan-review.png "Seeded demo: read the proposed plan. Approve plan has not been clicked. Not a live CLI result.")

If the ticket is unclear, the PM may ask 1 to 3 short questions first. The card shows “Waiting for your answers”. Answer in the ticket and click **Send answers** before you approve.

**Skip planning**, under **Status**, is the exception. Checking it saves immediately and leaves the ticket in its current column. Set **Status** to **Ready** and click **Save metadata**, or drag the card to **Ready**. Entering **Ready** starts the Technical Lead when one is available and the repository is ready. Without an approved plan, and with the box off, Coppice refuses the move: “Approve the plan, or turn on Skip planning, before moving this ticket to In Progress.” The Ready refusal is “Approve the plan before moving this ticket to Ready, or turn on Skip planning.” If no Technical Lead is available, the ticket stays in **Ready**, unassigned, and a comment says so.

## What continues without you

Leave `auto_start_runs` at `true` unless you want to start each later run yourself. After the Technical Lead finishes, the implementer starts, then review, then QA. You do not click before each of those runs.

On the usual path the ticket goes to **In Progress** on its own branch, then **In Review**, then **In QA**, then **Wait for Human Review**. Your main checkout stays untouched until you merge. Set `auto_start_runs` to `false` when those later runs should wait for **Run Agent**. A missing reviewer or QC agent leaves the ticket unassigned in that column and posts a comment. After a clarification round, an implementer who finishes from **In Progress** can reach **Wait for Human Review** without **In QA**.

## Check the diff before you accept

Open the ticket and click **Review code**. The page says **Code review**. Use this pass before **Accept**.

![Code review of the seeded file docs/greeting.md. The diff adds the line “Hello from the review demo.” Submit review is visible, and Accept is not on this page.](/blog/review-before-merging/diff-before-accept.png "Seeded demo: the greeting diff before Accept. Accept was not clicked, and the ticket stayed in Wait for Human Review. Not a live CLI result.")

The frame shows that seeded diff and the **Changed files** list. It does not show the **Approved plan** sidebar.

1. **Compare the change with the approved plan.** That sidebar is present only while the ticket is in **Wait for Human Review** and an approved plan is saved. It sits beside **Changed files** and the diff. Read each acceptance criterion and any evidence under it, then find that behavior in the code. **Met**, **Not met**, and **Not checked** are the agent's report, not your sign-off. A **Met** line still needs you to look. The steps under the plan are a reading list. Their boxes stay unchecked.
2. **Inspect files the plan did not name.** A changed file can be marked **Not in the plan**. Open those first. A lockfile, a migration, or a rename can belong with the work, or it can be scope the plan never asked for. When **Reviewer brief** is on the page, it matches this commit. **Changes from the plan** says what shifted, or **None**. **Look here first** names a file. The brief also says: “The agent wrote these notes when it finished. Coppice tags any file that wasn't in the plan you approved.”
3. **Judge correctness and the error paths.** Read what the change does when input is missing, a call fails, or a value is empty. Use the plan's risks and acceptance criteria as the list. Where the code is wrong, click the line number, type in “Leave a comment…”, and click **Add comment**. That note stays on the diff until you submit the review.
4. **Review permissions and dependencies.** In the diff, look for new packages, scripts, credentials, and checks that decide who can read or change data. Agents run with the permissions of the CLIs on your machine. This pass is where you catch an access change or a new dependency before it reaches your base branch.
5. **Read the tests you can see, and name the gaps.** Open the tests in the diff. Read the QC comments on the ticket when that run happened. **Not checked** means you have no result for that criterion. **Not met** means the agent reported a miss. A sentence in a comment is not a substitute for a test you can find. If the gaps are acceptable, say that in the review summary. If they are not, send the work back before you accept.

**Submit review** posts one comment on the ticket. **Summary** is required. **Comment only** leaves the column where it is and does not start a run. **Move to In Progress** returns the ticket to **In Progress** and, when `auto_start_runs` is `true`, starts the assigned engineer. The posted comment lists your line notes under **Inline comments**.

## Accept records the commit

**Accept** is on the ticket only in **Wait for Human Review**. It opens **Accept this ticket?** **Cancel** is focused. The dialog says “This accepts the work currently in Wait for Human Review.” Choose **Accept** when the checklist above is done.

Coppice records the tip of the ticket branch, moves the ticket to **Done**, and posts “Accepted: ticket moved to **Done**.” A later commit sends the ticket back to **In Review**. You accept the new tip before it can merge. **Accept…** is `Mod+Shift+Enter`. [Shortcuts](/docs/shortcuts) lists the keys.

## Merge when you choose

A **Done** card with a current acceptance shows “Accepted, not merged” and **Merge…**. **Merge…** opens **Merge ticket branch**. Coppice merges the commit you accepted into the **Base branch** you pick. A different tip is refused: “Not merged. This isn't the commit you reviewed.”

![Demo board with a Done card titled “Note a setting in the demo checkout.” The card reads Accepted, not merged and shows a separate Merge… button.](/blog/review-before-merging/done-card.png "Seeded demo from an earlier capture: Accept is already recorded, and Merge… is still separate. Not a live CLI result.")

The board figure is from an earlier seeded demo. It was not recaptured with the plan and diff figures above.

If **Merge…** conflicts and Coppice can abort the merge cleanly, the base branch stays as it was and the message names the files. The ticket stays in **Done** until you choose **Ask [agent] to resolve**. That moves the ticket to **In Progress** and starts a work run. **Accept** is refused there. When that run succeeds, the ticket is in **In Review**. That return is still before the next **Accept**. **Accept** is offered again only in **Wait for Human Review**. Record the new commit there before you merge.

**Create PR via API** is the other path. It needs a GitHub remote and a forge token. A merge on GitHub.com is recorded by a sweep at startup and every 5 minutes. Other remotes keep “Accepted, not merged” until you merge in Coppice. [FAQ](/docs/faq) states the same gate.

## Read next

[Getting started](/docs/getting-started) sets up the first ticket. [Concepts](/docs/concepts) is the column sequence.
