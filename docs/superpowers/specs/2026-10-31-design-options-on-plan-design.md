# Design options on the plan

Amends [the design preview plugin](2026-10-30-design-preview-plugin-design.md).

## Why

A planning agent can now publish design options, but on a ticket the card only appears in the Agent Runs tab. The plan on the Detail tab says "design option A, recommended" with nothing to look at. The plan is unreadable without the design it refers to.

The agent's recommendation is also only words in the plan, so Coppice cannot show it or fall back to it.

## Goals

- Someone reviewing a plan sees that design options exist, and can open and compare them from the Detail tab.
- The agent marks one option as recommended when it publishes.
- If nobody chooses, work follows the recommended option.
- Choosing a different option during Plan Review updates the plan without a separate request.

## Non-goals

- Blocking plan approval until someone chooses. Planning instructions stay guidance, and publishing stays optional. Approve only waits while a plan rewrite is running.
- Pausing an agent run to wait for a choice.
- Changing a design once work is underway, other than through a normal ticket comment.
- New design tools for agents beyond the recommendation field.

## Anti-goals

- A plan that describes one option while the ticket says another was chosen.
- A choice that looks applied but was not.
- An agent run starting without the button saying so first.
- A design changing underneath an agent that is already building.

## Behaviour

### Recommendation

`design__publish` gains a required `recommendedOptionId`, which must match one of the published options. Stored as `design_sets.recommended_option_id` (nullable, because sets published before this change have none).

The **selected** option is the human's choice if there is one, otherwise the recommended option. API responses, `design__list` and `ticket_get` return `recommended` and `selected`, each with id and title. Sets from before this change have `recommended: null`, and `selected` is the human's choice or null.

The design skill and `PLAN_DESIGN_INSTRUCTION` tell the agent to set the recommendation and write the plan around it. A plan revision does not publish new options unless the change request asks for them. Ticket work follows `selected`, so the work-run guidance says so whenever the agent has the design skill.

`design__list` and `ticket_get` also give the absolute file path of the `selected` option's HTML, so a building agent can read the mockup. Planning must confirm each connector can read the artifacts folder on a work run. A connector that cannot still gets the title and the plan text.

The plan records which option it was written around (`ticket_plans`, or an equivalent the builder picks). A plan is written around `selected` at the moment its run started.

### Sub-tickets

When a plan splits, each sub-ticket inherits the parent's selected design. `design__list`, `ticket_get` and the sub-ticket's Detail card use the parent's set. The card is read-only on the sub-ticket, with no Choose.

### After approval

The set the approved plan was written around stays on the Detail card and stays final, even if a later run publishes. A work run can still publish, usually because someone asked in a comment. That set appears on its Agent Runs card, a pick on it is saved, and it never changes the plan. It does not supersede the plan's set, and it replaces what `selected` means for later work only when someone picks an option on it.

### Card on the Detail tab

When a ticket has a published set that is not superseded, the Detail tab shows one card directly above the plan, in any ticket status that shows a plan, and in the same position when none is shown:

- Title: "Click to open design options"
- Detail: "3 options · Using A: Quiet button after the last card (recommended)". The wording says "(your choice)" when a human chose. Sets from before this change with no choice say "3 options", and so does a set whose option doesn't drive work: a work-run set nobody picked, or a plan-run set once someone picked on a work-run set. When `planOutOfDate` is true the card adds "The plan wasn't updated to this option."
- Button: Open

The card on the Agent Runs tab stays, and opens the same popup. Superseded sets stay reachable only from there.

### Popup

Opening a card shows a modal with one tab per option, the Recommended badge on the recommended option, a marker on the chosen one, and the existing sandboxed preview. Sandbox and blob URL rules do not change.

The footer shows what happens next:

| State | Footer |
| --- | --- |
| Viewing the selected option, plan reflects it, in Plan Review | "Work follows this option." with no action |
| Viewing the selected option, plan does not reflect it | "The plan wasn't updated to this option." with a "Try again" button |
| Viewing another option, plan run not active, Plan Review | Button "Choose B and update plan" |
| Plan run active | Text "The plan is being updated." with choosing disabled |
| Plan approved | "The plan is approved, so this design is final. To change it, add a comment on the ticket." with no action |
| No plan yet, plan run not active | Button "Choose B" |
| Plan-run set with no plan yet, viewing the selected option | "Work follows this option." with no action |
| Work-run or chat set, viewing the human's choice | "Work follows this option." with no action |
| Work-run or chat set, any other option, including the recommended one | Button "Choose B" |
| Set from before this change with nothing selected | Button "Choose B" |
| Sub-ticket | No action |

### Approving during a rewrite

`PlanningService::approve` is refused while a plan run is queued or running, and the Approve button is disabled with the existing plan-running wording. This is a new guard on existing behaviour and applies to every plan rewrite, not only design ones.

### Choosing

The server decides all of this in one operation behind `POST /api/designs/{setId}/choose`. The SPA does not chain requests.

For a set published by a `plan_ticket` run:

1. Picking the option that is already selected saves the choice if none was stored, and starts nothing.
2. Picking a different option in Plan Review saves the choice and records a change request through the existing `PlanningService::ask_for_changes` flow, in the same transaction, with the note "Update the plan to use design option B: Full-width button with Show all." If the set has no recommendation and no earlier choice, the pick saves and starts nothing.
3. While a plan run is active, the choice is refused and nothing is saved, using the existing `PLAN_ALREADY_RUNNING` message.
4. With no planner available, the choice is refused and nothing is saved, using the existing `NO_PM` or `NO_PLANNER` message.
5. With no plan yet, because the run asked questions or failed, the choice is saved and starts nothing. The next plan is written around it.
6. Once the plan is approved, the choice is refused and nothing is saved, using the approved-plan message above.
7. If a rewrite failed or was cancelled, the choice stays and the popup offers Try again, which requests the same change again. Compare `selected` with the option the current plan reflects to know this.

Sets published by other runs, and sets in chat, keep today's behaviour: the pick is saved and nothing else happens.

Choosing is refused for a superseded set, so a stale popup cannot change the plan.

## Constraints

- Server owns state: the rules live in `DesignService` and `PlanningService`, and the popup renders what the server allows.
- User-facing strings go in the existing copy modules.
- Migration is additive and sequential.
- The preview's security model is unchanged.

## Testing

- Publish without a valid `recommendedOptionId` is rejected.
- `selected` follows the choice, then falls back to the recommendation, and is exposed through the API, `design__list` and `ticket_get`.
- Picking the selected option starts no plan run. Picking another during Plan Review starts one with the note. Picking back to the earlier option starts another.
- Refused while a plan run is active, after approval, with no planner, and on a superseded set, with nothing saved.
- Approve is refused while a plan run is queued or running.
- A failed rewrite keeps the choice and offers Try again, which starts a new run.
- With no plan yet, a pick saves, starts nothing, and the next plan run sees it.
- A sub-ticket sees the parent's selected design and the mockup path, and cannot choose.
- A work-run publish leaves the approved set on the Detail card, and a pick on it starts no plan run.
- Sets from chat and non-plan runs save without a plan run.
- SPA: the card shows above the plan and opens the popup, and each footer state renders.

## Calls made while writing this

These were decided without asking, and can be overruled:

- The popup replaces the existing sidebar everywhere, including chat, so there is one preview UI.
- "Different" is compared with the currently selected option, not the original recommendation, so going from B back to A also updates the plan.
- Sets from before this change have no recommendation, and a first pick on one saves without changing the plan.
- `recommendedOptionId` is required in chat too, for one rule across tools.
- With no plan yet, a pick saves without starting a run, and the next plan is written around it.
- The plan's design is recorded when the plan is inserted, because picks are refused while a plan run is active. A pick and its plan rewrite request share one transaction, which commits only when the run starts.
- Plain "Choose B" wording for work-run, chat and older sets, where nothing else happens.
- A failed rewrite does not block Approve. The card and a line beside Approve say "The plan wasn't updated to this option." If someone approves anyway, work follows the chosen option, through `selectedDesign`, while the approved plan text still describes the earlier one.
- A work-run or chat set's recommendation drives nothing until someone confirms it, so those sets offer "Choose" on the recommended option too.
- When change requests conflict, as they do after picking back, the later one wins and the design to use is `selectedDesign` in `ticket_get`. A note identical to the last one is not appended again.

## Left to the builder

Component structure, how the card and popup share state, and how the card refreshes after a choice or a finished run.
