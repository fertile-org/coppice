# Design Options on the Plan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show published design options on the ticket Detail tab in a popup, record the agent's recommendation, follow it when nobody chooses, and update the plan when someone chooses differently.

**Architecture:** `DesignService` gains the recommendation, a server-computed choice state, and inheritance for sub-tickets. A new `design_choice` service combines it with `PlanningService` so a pick is one server operation. The SPA renders the state the server returns and invents no rules.

**Tech Stack:** Rust (Axum, SQLx, Postgres), React, TanStack Query, Radix dialog (`web/src/components/ui/dialog.tsx`), Vitest.

**Spec:** `docs/superpowers/specs/2026-10-31-design-options-on-plan-design.md`

## Global Constraints

- Server owns state: the SPA does not chain requests or decide what a pick does.
- Server copy goes in `server/src/copy/plan.rs`; plan UI copy goes in `web/src/features/plan/copy.ts`.
- Migrations are additive and sequential. The next number is `059`; confirm with `ls server/migrations` first.
- The iframe sandbox, blob URLs and CSP do not change.
- Publishing stays optional. Approve only waits while a plan run is queued or running.
- Fast checks only while iterating: targeted `cargo test -p coppice-server --features embedded-test-db --test integration_designs <filter>`, `make web-test`. `make test` only in the last task.
- Do not run `cargo fmt` across the crate. Format only lines you touch.

## Review Focus

- Two people or a double click pick options at once: the second is refused with `PLAN_ALREADY_RUNNING` and saves nothing. Test in Task 3.
- Starting the rewrite fails after the pick was saved: the earlier choice is restored. Test in Task 3.
- A popup left open while the plan is approved in another window: the pick is refused and the choice is unchanged. Test in Task 3.
- An archived ticket: the popup is read-only and a pick is refused. Test in Task 3.
- A sub-ticket whose parent has no designs: no card and no error, and `ticket_get` returns no selected design. Test in Task 4.

---

### Task 1: Store and require the recommendation

**Files:**
- Create: `server/migrations/059_design_recommendation.sql`
- Modify: `server/src/services/design_service.rs`, `server/src/mcp/tools/design.rs`, `server/src/mcp/design_source.rs`, `server/src/mcp/tools/tickets.rs`, `server/src/api/designs.rs`
- Modify: `server/builtin-plugins/design/skills/design-ui-options/SKILL.md`, `server/src/copy/plan.rs`
- Test: `server/tests/integration_designs.rs`

**Interfaces:**
- Produces: `DesignService::publish(set_id: Uuid, run_id: Uuid, options: Vec<DesignOption>, recommended_option_id: &str) -> Result<Uuid, DesignError>`
- Produces on `DesignSetSummary`: `recommended: Option<ChosenOption>`, `selected: Option<ChosenOption>`, `superseded: bool`, `from_plan_run: bool`

- [ ] **Step 1: Write the failing tests**

Update the `publish_html` helper and existing publishes to pass `recommendedOptionId`. Add:

```rust
#[tokio::test]
async fn publish_rejects_unknown_recommendation() {
    // publish with recommended_option_id "z" on options a, b
    // expect DesignError::Invalid("recommendedOptionId must be one of the options")
}

#[tokio::test]
async fn selected_follows_choice_then_falls_back_to_recommendation() {
    // publish a, b recommending "b"
    // summary: recommended.id == "b", selected.id == "b", chosen == None
    // choose "a": selected.id == "a", recommended.id == "b"
    // design__list and ticket_get both expose recommended and selected
}

#[tokio::test]
async fn set_from_before_recommendations_has_no_selection() {
    // UPDATE design_sets SET recommended_option_id = NULL
    // summary: recommended None, selected None
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs -- recommendation selected`
Expected: compile failure on the new `publish` argument.

- [ ] **Step 3: Implement**

- Migration: `ALTER TABLE design_sets ADD COLUMN recommended_option_id TEXT;`
- `publish` validates the id before the transaction and stores it. `recommendedOptionId` is required in the `design__publish` schema and the tool handler.
- `from_plan_run` comes from joining `agent_runs.job_type = 'plan_ticket'`; `superseded` is `superseded_by IS NOT NULL`.
- Add `recommended` and `selected` to `design__list`, `ticket_get` and `DesignSetResponse` (camelCase).
- Skill body gains: "Pass `recommendedOptionId` with the option you recommend. On a plan revision, do not publish new options unless the change request asks for them."
- `PLAN_DESIGN_INSTRUCTION` becomes: "If this ticket changes what people see, load `design:design-ui-options` and publish 2–3 design options before you submit the plan, marking the one you recommend, then write the plan around it and say which one it is. If `design__list` shows a selected option, write the plan around that one and don't publish new options unless a change request asks for them. Skip this when the ticket has no visible change."

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add server/migrations/059_design_recommendation.sql server/src server/builtin-plugins server/tests/integration_designs.rs
git commit -m "feat: record the agent's recommended design option"
```

### Task 2: Refuse Approve during a plan run, and record what the plan reflects

**Files:**
- Create: `server/migrations/060_plan_design_option.sql`
- Modify: `server/src/services/planning_service.rs`, `server/src/workers/job_worker/plan.rs`, `web/src/features/plan/PlanReviewActions.tsx`, `web/src/features/plan/copy.ts`, `web/src/features/tickets/TicketDetailPanel.tsx`
- Test: `server/tests/integration_plan_review.rs`, `web/src/features/plan/PlanReviewActions.test.tsx`

**Interfaces:**
- Consumes: `DesignSetSummary.selected` from Task 1.
- Produces: `ticket_plans.design_set_id UUID`, `ticket_plans.design_option_id TEXT`, on `TicketPlan` as `design_set_id: Option<Uuid>`, `design_option_id: Option<String>`.
- Produces: `PlanningService::planner_available(&self, ticket_id: Uuid) -> Result<bool, PlanError>` (false for `skip_planning` or no planner, without posting a note), and `pub` on `active_plan_run`.
- Produces: `PlanReviewActions` prop `planRunActive: boolean`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn approve_is_refused_while_a_plan_run_is_active() {
    // Plan Review with a plan and a queued plan_ticket run
    // approve returns PlanError::Validation(PLAN_ALREADY_RUNNING); approved_plan_id stays NULL
}

#[tokio::test]
async fn inserted_plan_records_the_selected_design() {
    // plan run publishes a, b recommending b, then insert_plan
    // latest_plan: design_set_id == set, design_option_id == Some("b")
}
```

```tsx
it('disables Approve while a plan run is active', () => {
  render(<PlanReviewActions {...props} planRunActive />);
  expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_plan_review -- approve_is_refused inserted_plan_records`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Migration adds both nullable columns; `design_set_id` references `design_sets(id) ON DELETE SET NULL`.
- `approve` checks `active_plan_run` right after the status check. Fix any existing test that approved during a queued run by finishing that run first.
- `insert_plan` records the newest published, non-superseded plan-run set and its `selected`. Reading at insert is correct because picks are refused during a plan run, and it covers options published by the run itself.
- `TicketDetailPanel` derives `planRunActive` from `useAgentRuns` (`jobType === 'plan_ticket'`, status queued or running).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_plan_review` and `make web-test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add server/migrations/060_plan_design_option.sql server/src server/tests/integration_plan_review.rs web/src/features
git commit -m "feat: hold plan approval during a rewrite and track the plan's design"
```

### Task 3: Make a pick one server operation

**Files:**
- Create: `server/src/services/design_choice.rs`
- Modify: `server/src/services/mod.rs`, `server/src/api/designs.rs`, `server/src/copy/plan.rs`
- Test: `server/tests/integration_designs.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces:

```rust
#[serde(rename_all = "snake_case")]
pub enum ChoiceMode { UpdatePlan, SaveOnly, PlanRunning, PlanFinal, ReadOnly }

pub struct ChoiceState { pub mode: ChoiceMode, pub plan_out_of_date: bool }

pub async fn choice_state(pool: &PgPool, set: &DesignSetSummary, ticket_id: Option<Uuid>, inherited: bool)
    -> Result<ChoiceState, DesignChoiceError>;
pub async fn choose_design(pool: &PgPool, artifacts: &Path, set_id: Uuid, option_id: &str, user_id: Uuid)
    -> Result<(), DesignChoiceError>; // Design(DesignError) | Plan(PlanError)
```

- API: `DesignSetResponse` gains `choice: { mode, planOutOfDate }`. `POST /api/designs/{setId}/choose` returns refusals with `map_plan_error`'s response shape, so the SPA can show the message.

- [ ] **Step 1: Write the failing tests**

```rust
// each in integration_designs.rs, on a Plan Review ticket with a plan-run set recommending "a"
async fn picking_the_selected_option_starts_no_run() {}
async fn picking_another_starts_a_run_with_the_note() {
    // change request contains
    // "Update the plan to use design option B: Full-width button with Show all."
}
async fn picking_back_to_the_earlier_option_starts_another_run() {}
async fn pick_is_refused_during_a_plan_run_and_saves_nothing() {} // PLAN_ALREADY_RUNNING
async fn pick_is_refused_after_approval_and_saves_nothing() {}    // PLAN_DESIGN_FINAL
async fn pick_is_refused_without_a_planner_and_saves_nothing() {} // NO_PM, and no note posted
async fn pick_is_refused_on_a_superseded_set() {}                 // DESIGN_SUPERSEDED
async fn pick_is_refused_on_an_archived_ticket() {}               // mode ReadOnly
async fn failed_start_restores_the_earlier_choice() {}
async fn failed_rewrite_keeps_choice_and_try_again_starts_a_run() {
    // plan_out_of_date true; re-picking the selected option starts a run
}
async fn pick_with_no_plan_saves_and_starts_no_run() {}
async fn pick_on_a_set_from_before_recommendations_saves_and_starts_no_run() {}
async fn work_run_and_chat_sets_save_and_start_no_run() {}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs -- pick`
Expected: FAIL.

- [ ] **Step 3: Implement `choice_state`, checked in this order**

1. Inherited, superseded, or archived ticket: `ReadOnly`.
2. Chat or not `from_plan_run`: `SaveOnly`.
3. `tickets.approved_plan_id` set, or status past Ready: `PlanFinal`.
4. Active plan run: `PlanRunning`.
5. No plan yet, or status not `PlanReview`: `SaveOnly`.
6. Otherwise `UpdatePlan`.

`plan_out_of_date` is true when the mode is `UpdatePlan`, `selected` exists, the latest plan's `design_set_id` is set, and it differs from this set and `selected`. A plan with no recorded design counts as up to date.

- [ ] **Step 4: Implement `choose_design`**

Validate the option. Match on the mode:

- `ReadOnly`: refuse with `DESIGN_SUPERSEDED` or the existing archived error.
- `PlanRunning`: refuse with `PLAN_ALREADY_RUNNING`.
- `PlanFinal`: refuse with `PLAN_DESIGN_FINAL`.
- `SaveOnly`: save.
- `UpdatePlan`: if `selected` is `None`, save. If the pick equals `selected` and the plan is up to date, save only. Otherwise check `planner_available` (refuse with `NO_PM` when false), save the pick, then call `ask_for_changes` with `plan_design_change_note(title)`. If that errors, restore the earlier `chosen_*` values and return the error.

`ask_for_changes` uses the pool and queues a run, so this is not one database transaction. The run must see the saved pick, so the save comes first and is undone on failure.

Copy in `server/src/copy/plan.rs`:

```rust
pub const PLAN_DESIGN_FINAL: &str = "The plan is approved, so this design is final. To change it, add a comment on the ticket.";
pub const DESIGN_SUPERSEDED: &str = "Newer design options replaced these. Open the latest ones to choose.";
pub fn plan_design_change_note(title: &str) -> String; // "Update the plan to use design option {title}."
```

- [ ] **Step 5: Wire the API and run the tests**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs`
Expected: PASS. Publish ticket and run events after a pick that starts a run, as `ask_for_plan_changes` does with `publish_ticket_and_runs`.

- [ ] **Step 6: Commit**

```bash
git add server/src server/tests/integration_designs.rs
git commit -m "feat: update the plan when a different design is chosen"
```

### Task 4: Follow the design through work runs and sub-tickets

**Files:**
- Modify: `server/src/services/design_service.rs`, `server/src/mcp/tools/design.rs`, `server/src/mcp/tools/tickets.rs`, `server/src/api/designs.rs`, `server/src/workers/job_worker.rs`, `server/src/workers/job_worker/plan.rs`, `server/src/copy/plan.rs`
- Test: `server/tests/integration_designs.rs`

**Interfaces:**
- Consumes: `selected`, `from_plan_run`, `superseded`, `ChoiceMode::ReadOnly`.
- Produces:

```rust
pub struct EffectiveDesign { pub set_id: Uuid, pub title: String, pub selected: ChosenOption, pub entry_path: PathBuf }
impl DesignService<'_> {
    pub async fn effective_for_ticket(&self, ticket_id: Uuid) -> Result<Option<EffectiveDesign>, DesignError>;
}
```

- `ticket_get` gains `selectedDesign: { setId, title, id, optionTitle, path } | null`; `design__list` gains the same as a top-level key.
- `DesignSetSummary` and `DesignSetResponse` gain `inherited: bool`.
- `has_design_skill` becomes `pub(crate)` in `job_worker/plan.rs`.

- [ ] **Step 1: Write the failing tests**

```rust
async fn work_run_publish_does_not_supersede_the_plan_set() {}
async fn work_run_pick_becomes_the_effective_design() {}
async fn plan_set_is_effective_when_no_work_run_set_was_picked() {}
async fn effective_design_path_is_the_absolute_entry_file() {}
async fn sub_ticket_inherits_the_parent_selected_design() {
    // list_for_ticket(child) includes the parent's set with inherited == true,
    // choice.mode == read_only; ticket_get(child).selectedDesign is set
}
async fn sub_ticket_of_a_parent_without_designs_has_none() {}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs -- effective inherit supersede`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `publish`'s supersede query only touches sets of the same kind: plan-run sets supersede plan-run sets, other sets supersede other sets.
- `effective_for_ticket`: newest non-superseded set from a non-plan run with a human choice; else newest non-superseded plan-run set with a `selected`; else the same lookup on `parent_ticket_id`, one level up.
- `list_for_ticket` appends the parent's set behind that result when the ticket has a parent, marked `inherited`.
- Add `WORK_DESIGN_INSTRUCTION` to `server/src/copy/plan.rs`: "If `ticket_get` returns `selectedDesign`, build that option. Read the HTML at its `path` for the layout when you can; otherwise follow its title and the plan." Append it to the context in `job_worker.rs` for `work_on_ticket` runs when `has_design_skill(&skills)`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_designs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add server/src server/tests/integration_designs.rs
git commit -m "feat: carry the selected design into work runs and sub-tickets"
```

### Task 5: Card on the Detail tab and the popup

**Files:**
- Create: `web/src/features/designs/DesignDialog.tsx`, `web/src/features/designs/DesignDialog.test.tsx`, `web/src/features/designs/DesignPlanCard.tsx`, `web/src/features/designs/DesignPlanCard.test.tsx`, `web/src/features/designs/designCopy.ts`
- Delete: `web/src/features/designs/DesignSidebar.tsx`
- Modify: `web/src/lib/schemas/design.ts`, `web/src/features/designs/DesignCard.tsx`, `web/src/features/designs/DesignCard.test.tsx`, `web/src/features/designs/useDesigns.ts`, `web/src/features/tickets/TicketDetailPanel.tsx`, `web/src/features/tickets/useTicket.ts`

**Interfaces:**
- Consumes: API fields from Tasks 1, 3, 4.
- Produces: `designSetSchema` gains `recommended`, `selected` (`chosenDesignOptionSchema.nullable()`), `superseded`, `fromPlanRun`, `inherited` (booleans), `choice: { mode, planOutOfDate }`.
- Produces: `<DesignDialog design scope onClose />` and `<DesignPlanCard ticketId />`.

- [ ] **Step 1: Write the failing tests**

```tsx
// DesignPlanCard.test.tsx
it('shows the card text', () => {
  // title "Click to open design options"
  // "3 options · Using A: Quiet button after the last card (recommended)"
});
it('says your choice when a human chose', () => {});
it('prefers the plan-run set and ignores superseded sets', () => {});
it('renders nothing without a set', () => {});
it('opens the popup', () => {});

// DesignDialog.test.tsx, one per footer state
it('selected option, plan up to date: "Work follows this option." and no button', () => {});
it('out of date: "The plan wasn\'t updated to this option." with Try again', () => {});
it('update_plan on another option: "Choose B and update plan"', () => {});
it('plan_running: "The plan is being updated." with no button', () => {});
it('plan_final: approved-plan text and no button', () => {});
it('save_only: "Choose B"', () => {});
it('read_only: no action', () => {});
it('shows the Recommended badge and the server error message', () => {});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd web && npx vitest run src/features/designs`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Move `DesignPreview` unchanged into `DesignDialog.tsx`, built on `components/ui/dialog.tsx`, per the approved mockup: option tabs, Recommended badge, chosen marker, preview, footer. Keep the iframe at a height that fits the dialog on a small window.
- `DesignCard` opens `DesignDialog` instead of the sidebar, so chat uses it too. Delete `DesignSidebar`, and move any of its tests.
- `DesignPlanCard` uses `useDesigns(ticketId)`, skips superseded sets, and prefers a plan-run set. Render it in `TicketDetailPanel` directly above `PlanReviewActions` and `PlanChecklist`, in every status.
- `useChooseDesign` also invalidates the ticket and agent-run queries, and exposes the server's message with `parseApiErrorMessage`. Remove the local `picked` state from `DesignCard`; the query is the source of truth.
- In `useTicket.ts`, invalidate `designsQueryKey(ticketId)` when an agent run for that ticket finishes, so a new card and footer appear without a reload.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `make web-test` and `cd web && npx tsc -b`
Expected: PASS with no type errors.

- [ ] **Step 5: Commit**

```bash
git add web/src
git commit -m "feat: show design options on the plan in a popup"
```

### Task 6: Docs, connector check and final acceptance

**Files:**
- Modify: `docs/plugins.md`, `docs/providers/README.md`, `docs/superpowers/specs/2026-10-31-design-options-on-plan-design.md`

- [ ] **Step 1: Check each connector can read the mockup path**

Against an installed CLI, ask a work-mode run to read a file under the artifacts folder. Record one line per connector in `docs/providers/README.md`: `claude-code`, `codex`, `cursor`, `opencode`, `kilo-code`, marking any CLI not installed as "Not yet". Do not change launch flags here: a connector that cannot read it falls back to the title and plan, as the spec allows. Report any that cannot.

- [ ] **Step 2: Update docs**

`docs/plugins.md`: the card, popup, recommendation and plan update. Spec, in "Calls made": a pick saves first and is undone on failure instead of sharing a transaction, and the plan's design is recorded when the plan is inserted, since picks are refused during a run.

- [ ] **Step 3: Run the full checks**

Run: `make test`, `cargo clippy --workspace -- -D warnings`, `cargo clippy --workspace --features mock-provider -- -D warnings`, `cargo check --locked --workspace --all-targets`, `make web-test`, `cd web && yarn build`
Expected: all pass. Then run `make clean`.

- [ ] **Step 4: Check it in the real app**

Run planning on a UI ticket. Confirm the card shows above the plan, the popup previews all options, choosing another option starts a rewrite with Approve disabled, the new plan names that option, and approving locks the popup.

- [ ] **Step 5: Commit**

```bash
git add docs
git commit -m "docs: describe design options on the plan"
```
