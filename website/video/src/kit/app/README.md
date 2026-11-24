# kit/app: recreated Coppice UI

Simplified copies of the app's UI, first built for the Plan Review post. Dark theme, tokens only,
props only. Paths are in the app repo (`../coppice`), checked at commit
`92bcdbe0` (same as `TOKENS_SOURCE_COMMIT`; refreshed from `5ee07c09` for the Plan Review post:
Usage nav item, Skip planning switch row, Blocked column removed, chart tokens added; refreshed from
`6507a313` for the Model per role post: the app's `199a2a8e` wraps every Tailwind colour in
`withAlpha` (`color-mix(... calc(<alpha-value> * 100%), transparent)`), so opacity modifiers such as
`border-accent/40` now draw in the app, and the kit uses them as the app does).

The marketing kit lays the app out at a logical window of 1280×800 inside
`AppShell` and scales ×1.125 to fill 1440×900 (`APP_LOGICAL` / `VIDEO`). All
sizes inside are the app's own Tailwind classes at that logical size. (The X
posts in `coppice-content` still use 1280×720 ×1.5 → 1920×1080.)

## Pieces

| Piece | Mirrors | Checked against |
| --- | --- | --- |
| `AppShell` | `web/src/components/AppShell.tsx` (admin, desktop app: Users hidden, System and Help closed; `active` nav item, Boards by default); `features/keyboard/SidebarSearch.tsx`; `features/theme/ThemeToggle.tsx` (dark); `features/notifications/NotificationBell.tsx` (no unread) | `static/screenshots/07-board.png` |
| `Board` | `web/src/features/board/BoardPage.tsx` (header, Filter), `BoardColumn.tsx` (frame, count, Backlog quick add), `columns.ts` | `07-board.png`, `03-approve.png` |
| `TicketCard` | `web/src/features/board/TicketCard.tsx` (title, live dot, assignee, priority pill, plan-state line, Run agent) | `07-board.png`, `03-approve.png` |
| `PlanPanel` | No plan panel exists in the app: the plan is reviewed in the ticket drawer. Mirrors `features/tickets/TicketDrawer.tsx` (sheet, header, tabs), `TicketDetailPanel.tsx` (title, Edit, description, Attach files), `features/plan/PlanReviewActions.tsx` (Proposed plan, Approve plan), the Assignee and Status fields of `TicketMetadataPanel.tsx`. `approved` shows the state after approval: status Ready, review section gone, plan written into the description under `## Approved plan` (`server/src/copy/plan.rs`, `approved_plan_section_body`) | `02-plan.png` (approved state: no screenshot; from the source) |
| `DesignDialog` | `web/src/features/designs/DesignDialog.tsx` in `web/src/components/ui/dialog.tsx` (DialogContent, showCloseButton): header, option tabs with Recommended / Chosen pills, preview, footer (`footerFor`: "Work follows this option.", "Choose X and update plan", `plan_running` "The plan is being updated."). Preview pages redrawn light, layout, text and colours from `e2e/screenshot/design-options.mjs` | `static/screenshots/08-design-options.png` |
| `PlanPanel` `design` | `web/src/features/designs/DesignPlanCard.tsx` (card title, "N options · Using X: … (recommended / your choice)", Open), placed where `TicketDetailPanel.tsx` renders it; `planRunActive` and `changeRequests` from `features/plan/PlanReviewActions.tsx` | `02-plan.png` (card) |
| `CodeReview` | `web/src/features/code/CodeReviewPage.tsx` (header, Worktree / Base selects, Split / Unified, Hide whitespace, three-column body for a ticket in Wait for Human Review with an approved plan), `CodeReviewBackButton.tsx`, `features/plan/PlanChecklist.tsx` (Approved plan, steps), `ChangedFilesPanel.tsx`, `FileReviewHeader.tsx`, `DiffViewer.tsx` (unified react-diff-view table with the `.code-review-diff` colours from `web/src/index.css`, highlight.js token colours, `InlineCommentWidget` compose state, `PendingCommentBubble`). Outside `AppShell`, as the app's `/code` route is | `static/screenshots/05-diff.png` (diff, comment bubble, plan and file columns; its header is an older layout, the kit follows the current source) |
| `SubmitReviewDialog` | `web/src/features/code/SubmitReviewDialog.tsx` (linked ticket: Summary, Workflow action, Engineer, Preview via `formatReviewPreview.ts`, Cancel / Submit review) in `components/ui/dialog.tsx`; the open Workflow action list from `components/ui/combobox.tsx`, `popover.tsx`, `command.tsx` | no screenshot; from the source |
| `AgentsPage` | `web/src/features/agents/AgentsPage.tsx` (header, New agent; the agents table: thead, AgentRow with the preset key under the name, connector id with the mono `{provider}/{model}` line, Enabled and Healthy pills, Updated, Disable / Edit; `editHover` = the row's and Edit's hover colours) and `KnowledgeCompactionCard.tsx`, which the page always renders above the table once agents load (simplified: admin view, compaction off, Save disabled, status line). `scrollTop` = the `<main>` scroll | `website/public/assets/docs/new-agent.webp` (table behind the dialog); otherwise from the source |
| `EditAgentDialog` | `EditAgentDialog` in `web/src/features/agents/AgentsPage.tsx` with `AgentForm.tsx` (edit mode) in `components/ui/dialog.tsx` (`max-h-[90vh] max-w-3xl`): Edit agent, "Update {name}'s configuration.", Name, Role, Skills (no enabled plugins), Responsibilities, System prompt (rows 20), Connector, Model with Refresh models (`RefreshModelsButton.tsx`), Thinking level (only for a connector whose `caps.thinkingLevel` is true: Claude Code, `connectors/src/lib.rs` `takes_thinking_level`), Enabled; footer Cancel / Save changes, `saving` = isPending (Saving…, both disabled). `scrollTop` = the form body's scroll; `connectorList` / `modelList` = the open combobox popover | `website/public/assets/docs/new-agent.webp`, `model-picker.webp` (layout; the kit follows the current source) |
| `ComboboxTrigger`, `ComboboxList` | `web/src/components/ui/combobox.tsx` with `popover.tsx`, `command.tsx`: the closed trigger (`field-control`), the open list (search input only above 5 options, `max-h-72`, highlighted item, Check on the value, `side` = Radix's flip above the trigger). Shared by `SubmitReviewDialog` and `EditAgentDialog` | `model-picker.webp` |
| `Toast` | `web/src/components/ToastProvider.tsx` (region top-right, one success message with Dismiss and the 3 s progress bar) | no screenshot; from the source |
| `PlanPanel` `status`, `activeRun` | `features/tickets/TicketDrawer.tsx` header while a run is active (Run Agent and Archive disabled, Stop), `TicketStatusBadge.tsx` for a later status | `02-plan.png` (header layout) |
| `Button` | `web/src/components/ui/button.tsx` (`primary` = app `default`, `secondary`, `destructive`, `ghost`, size `sm`, `disabled` = the app's dashed disabled style, `loading` = Loader2 spinner). `pressed` = the app's hover colours | `02-plan.png` header buttons |
| `Badge` | `web/src/components/ui/badge.tsx`; with `status`, `features/tickets/TicketStatusBadge.tsx` | `02-plan.png` status pill |
| `Markdown` (internal) | the headings / paragraph / list / task-item subset of `web/src/components/TicketMarkdown.tsx` | `02-plan.png` plan box |

`copy.ts` holds the verified strings as constants; posts should take labels from it.

## Verified strings

Columns, in app order (`web/src/features/board/columns.ts`, `BOARD_COLUMNS`; "Plan Review"
is `PLAN_COPY.column` in `web/src/features/plan/copy.ts`):
Backlog, Plan Review, Ready, In Progress, In Review, In QA, Wait for Human Review, Done.
(There is no Blocked column since `6507a313`: blocked is a substatus pill on the card.)

Plan review (`web/src/features/plan/copy.ts`, `PLAN_COPY`):
- Approve button: "Approve plan" (while pending: "Approving…")
- "Ask for changes", placeholder "What should change?"
- "Skip human review", "Skip planning", "Goes straight from Backlog to Ready without a plan." (Skip planning's help line under its switch, `TicketMetadataPanel.tsx`)
- "Requested changes"
- "Proposed plan", "Revision 1"
- "Approving adds this plan to the description, under Approved plan."

Ticket card states around planning (`web/src/features/plan/copy.ts`, chosen by
`web/src/features/plan/planCardState.ts`):
- "Writing plan" (plan run queued or running)
- "Waiting for your answers" (Plan Review, PM asked questions)
- "Waiting for you to approve the plan" (Plan Review)
- "Plan needs a run" (Backlog, or Ready without an approved plan), with button "Run agent" (or "Assign PM" when a PM is suggested)
- Ready with an approved plan: no line.

Flow: Backlog → Plan Review (PM writes the plan) → Approve plan → Ready
(`website/src/pages/docs/concepts.md`, `getting-started.md`).

Design options (`web/src/features/designs/designCopy.ts`, `DESIGN_COPY`; in `copy.ts` as `DESIGN`):
- Card: "Click to open design options", "{n} options", "Using {label}: {title} (recommended)" / "(your choice)", "Open"
- Dialog: "Design options", "Recommended", "Chosen", "{label}: {title}", "Choose {label} and update plan", "Work follows this option.", "The plan is being updated."

The design set on screen (`e2e/screenshot/design-options.mjs`, the set behind `08-design-options.png`;
in `copy.ts` as `DARK_MODE_DESIGNS`): set title "Dark mode toggle on Settings"; options
A "Switch in the Settings row" (recommended), B "Light, Dark and System buttons", C "Theme preview cards";
the preview pages' text (Settings, "Preferences for acme-web on this machine.", Dark mode, Theme,
Light / Dark / System, Language, English, Start page, Dashboard and their hints).

What a pick does (`server/src/services/design_choice.rs`, `choose_design`): on a plan-run set while the
ticket is in Plan Review, choosing an option the plan doesn't follow saves the choice and asks for plan
changes ("Update the plan to use design option {label}: {title}.", `server/src/copy/plan.rs`
`plan_design_change_note`), which starts a PM plan run. While it runs, the dialog footer reads "The plan is
being updated." and Approve plan is disabled (`PlanReviewActions.tsx`, `planRunActive`). The new plan
arrives as the next revision, with the request under "Requested changes" (`PLAN_COPY.requestedChanges`).

Plan markdown (`server/src/copy/plan.rs`): `## Plan`, `### Approach`, `### Steps`,
`### Acceptance criteria`, `### Risks`, `### Out of scope`; approved section `## Approved plan`.

Code review (`web/src/features/code/`; in `copy.ts` as `CODE_REVIEW`, `APPROVED_PLAN`, `worktreeLabel`):
- Header: "Back" (`CodeReviewBackButton.tsx`), "Code review", "into" (MergeLine), "Submit review", "Worktree", "Base", "Split", "Unified", "Hide whitespace" (`CodeReviewPage.tsx`); the Worktree label "{dir} · {branch} · {ticket title}" (`worktreeLabel`)
- "Approved plan" (`features/plan/copy.ts`, `PLAN_COPY.approvedPlan`, shown by `PlanChecklist.tsx`)
- "Changed files", "{n} of {total} viewed" (`ChangedFilesPanel.tsx`); status letter "M" (`statusLabel`)
- "Viewed", "‹ Prev" with "K", "Next" with "J" and "›", "1 / 1" (`FileReviewHeader.tsx`)
- "Click a line number to add an inline comment.", "Leave a comment…", "Add comment", "Cancel", "Edit", "Remove" (`DiffViewer.tsx`)

Submit review dialog (`web/src/features/code/SubmitReviewDialog.tsx`; `SUBMIT_REVIEW`, `formatReviewPreview`):
- "Submit review", "Post a combined review comment on the linked ticket.", "Summary", "Overall feedback for this change…",
  "Workflow action" with "Comment only" (default), "Move to In Progress", "Reassign engineer"; "Engineer" with "{name} · {role}";
  "Preview" (text from `formatReviewPreview.ts`, "(summary required)" when empty); "Cancel", "Submit review"; toast "Review posted"
- Engineer shown: agent "Frontend", role "Frontend Engineer" (the `frontend_engineer` preset, `server/migrations/002_workspace.sql`;
  the agent name the app's screenshots seed, `e2e/screenshot/capture-hero.mjs`); `FRONTEND_ENGINEER`
- Drawer while a run is active: "Stop" (`features/tickets/TicketDrawer.tsx`; `DRAWER.stop`)

The diff on screen (`THEME_TOGGLE_REVIEW`): `e2e/screenshot/seed-hero-repo.sh` (acme-web, `src/settings/ThemeToggle.tsx`, base
with only `SettingsHeading`, +15 −0) and `capture-hero.mjs` (ticket "Add a dark mode toggle to Settings", comment
"Use the existing theme hook." on line 8, plan steps), as in `05-diff.png`. Token colours are what the app's highlight.js
(typescript) gives the new file.

What submitting does (`server/src/services/code_review_service.rs` `submit_review`, `server/src/api/code_reviews.rs`): the review is
posted as one ticket comment (`format_review_comment`); with Move to In Progress the ticket goes to In Progress and is assigned the
dialog's engineer, which the dialog prefills with the ticket's current assignee when that agent is an engineer, so the same agent
keeps it; then `should_start_run` starts its run when `workflow.auto_start_runs` is on (the default, `config/src/lib.rs`). The
dialog closes itself on success, toasts "Review posted" and opens the board with the ticket's drawer (`navigate(/boards/…?ticket=…)`).

Agents page (`web/src/features/agents/`; in `copy.ts` as `AGENTS`, `KNOWLEDGE_COMPACTION`):
- Page: "Agents", "Configure your agent team from presets.", "New agent"; table headers "Name", "Role", "Connector", "Status",
  "Health", "Updated"; pills "Enabled", "Healthy"; row buttons "Disable", "Edit"; model line `{provider}/{model}` (`agentModelLine`)
  (`AgentsPage.tsx`)
- Knowledge compaction card (`KnowledgeCompactionCard.tsx`): "Knowledge compaction", "Agent that turns Done tickets into knowledge
  candidates for review. Leave empty to turn compaction off.", "Compaction agent", "None — compaction off", "Save"; status line
  "Runs every 30 min in batches of up to 10 tickets, read-only, no repo. 0 Done tickets are waiting." (`formatInterval`,
  `pluralTickets` in `features/knowledge/compactionFormat.ts`; defaults `interval_secs` 1800, `batch_max_tickets` 10 in
  `config/src/lib.rs`)
- Edit dialog: "Edit agent", "Update {name}'s configuration." (`AgentsPage.tsx`); "Name", "Role", "Responsibilities", "(one per line)",
  "System prompt", "Connector", "Model", "Cancel", "Save changes", "Saving…", "Search connectors…" (`AgentForm.tsx`); "Skills",
  "(plugins this agent can use)", "Thinking level" (`features/agents/copy.ts`); "No enabled plugins. Enable plugins on the Plugins
  page." (`AgentForm.tsx`); "Changes apply from each agent's next run." (`features/plugins/copy.ts`, `PLUGIN_COPY.nextRun`);
  "Refresh models" (`RefreshModelsButton.tsx`); "Enabled" (the checkbox, `AgentForm.tsx`)
- Connectors (`CONNECTORS`): option labels "{name} — {status}" (`AgentForm.tsx` `connectorOptionLabel`), statuses "Ready", "Not on your
  PATH" (`features/tools/connectorCopy.ts` `readinessLabel`); names and order Cursor, Claude Code, Codex, Kilo Code, OpenCode, GitHub
  Copilot, Qwen Code, Kimi Code, Grok and ids `claude-code`, `codex`, `opencode` (`connectors/src/lib.rs`); connectors that are on
  first (`connectorPickerOptions`), here Claude Code, Codex, OpenCode (Ready); "{connector}'s default" (`connectorCopy.ts`
  `CONNECTOR_DEFAULT_MODEL_LABEL`). OpenCode takes no thinking level (its run contract has none; only Claude Code does)
- Models (`MODELS`): Claude Opus 5.5 `opus/claude-opus-5-5`, Claude Sonnet 5.5 `sonnet/claude-sonnet-5-5`, Claude Haiku 5.5
  `haiku/claude-haiku-5-5` (`server/src/providers/models.rs`)
- OpenCode models (`OPENCODE_MODELS`): live from `opencode models` (`server/src/providers/opencode_models.rs`
  `parse_opencode_models_stdout`: the id is the line without `<provider>/`, the name equals the id, sorted by id; providers
  discovered from the same output, `models.rs` `OpenCodeModels`, `cli_catalog.rs` `discover_cli_providers`). One provider,
  `openrouter`, so no group headings (`AgentForm.tsx` `modelPickerOptions` `showGroups`) and, with four options, no search box.
  Ids `moonshotai/kimi-k3`, `qwen/qwen3.8`, `z-ai/glm-5.1` are example content; the table line is `openrouter/qwen/qwen3.8`
- Agents (`HERO_AGENTS`): PM (`pm`, role "PM"), Frontend (`frontend_engineer`, "Frontend Engineer"), QA (`qc`, "QC"): the agents
  `e2e/screenshot/capture-hero.mjs` seeds, roles from `server/migrations/002_workspace.sql`, ordered by `PRESET_ORDER`
  (`server/src/domain/agent.rs`)
- Frontend's form content (`FRONTEND_PRESET`): responsibilities from `server/migrations/008_agent_template_files.sql` and
  `037_fold_agent_skills_into_responsibilities.sql`; system prompt start from `server/agent_templates/frontend_engineer.md` with
  `{{posture}}` replaced by `server/agent_templates/posture.md`

What saving does (`EditAgentDialog` `handleSubmit`, `useUpdateAgent`): PATCH the agent, toast "{name} turned on." only when the
server reports `connectorTurnedOn` (not when the connector was already on), then `onClose()`: the dialog closes itself and the
table refetches with the new connector and model line. Picking a connector resets the model to the connector's default
(`handleConnectorChange`).

Other labels:
- Sidebar: Boards, Agents, Chat, Knowledge, Repositories, Plugins, Usage, System, Settings, Help, Collapse, "Search…", "Ctrl+K" (`web/src/components/AppShell.tsx`, `features/system/paths.ts`, `features/keyboard/SidebarSearch.tsx`, `features/keyboard/types.ts`)
- Top bar: theme toggle "dark" shown capitalised (`features/theme/ThemeToggle.tsx`)
- Board: "Board", "Drag tickets between columns to update status.", "Filter", "Add ticket…" (`features/board/BoardPage.tsx`, `BoardFilterDrawer.tsx`, `BoardColumn.tsx`)
- Drawer: "Ticket", "Run Agent", "Archive", "Close", tabs "Detail", "Live Console", "Agent Runs" (`features/tickets/TicketDrawer.tsx`, `features/keyboard/drawerSections.ts`)
- Detail: "Edit", "Attach files", "No description yet." (`features/tickets/TicketDetailPanel.tsx`, `AttachFilesButton.tsx`)
- Metadata: "Assignee", "Assigned to {name}", "{name} ({role})", "Status" (`features/tickets/TicketMetadataPanel.tsx`)

## Deviations from the app

- `CodeReview` columns are fixed at 240 / 280 px, the app's `minmax(200px,240px)` and `minmax(220px,280px)` at the 1280 px window.
  The diff font is the kit's `font-mono` token; the app's react-diff-view uses its own `Consolas, Courier, monospace`.
  Row heights, gutter widths (7ch) and padding are react-diff-view's own CSS. `ReviewerBriefBar` is not drawn (no brief in the
  app's screenshot fixture). The diff column's scroll is not modelled: rows past the window are clipped.
- `SubmitReviewDialog`: the body's scroll is not modelled (content past `max-h-[90vh]` is clipped, as the app shows it before
  scrolling); the Workflow action list is drawn inside the dialog instead of a portal.
- `Toast` and the dialog's `enter` stand for the app's CSS fade-in and dialog-in animations, driven by frame.
- Opacity-modified colours (`border-accent/40`, `border-border/60`, `bg-accent-muted/30`, `bg-accent-muted/50`,
  `hover:bg-danger-muted/40`): since the app's `199a2a8e` they draw in the app, and the kit uses the same classes. Tailwind v4
  replaces `<alpha-value>` with 1 and applies the modifier with its own `color-mix(in oklab, …)`, where the app's Tailwind 3.4
  puts the fraction in `<alpha-value>` (`color-mix(in srgb, …)`): the blend space differs slightly.
- `EditAgentDialog`: the form body's scroll is `scrollTop`; the open lists are drawn in the dialog instead of a portal, with the
  side (`top` for the connector list, which doesn't fit below its trigger) set by the post. The System prompt and Responsibilities
  textareas show their first lines, as the app does before they are scrolled. No plugins are enabled, so Skills shows the empty
  line. Thinking level shows "Claude Code's default" (no level set). The Enabled checkbox is the browser's own (Chrome blue, no `accent-color` in the app), drawn in `bg-info`.
- `AgentsPage`: `KnowledgeCompactionCard` shows only its admin view with compaction off; Updated dates are example content
  (`formatDate`, en-US).
- Textarea placeholder is the preflight `#9ca3af` in the app; the kit uses `text-text-secondary` (#a1a1aa).
- `field-control` and the combobox trigger are inlined as token styles.
- No grain texture, hover, focus, transition or pulse classes.
- `PlanPanel` reveals the plan line by line (`revealedLines`); the app shows it whole when the run ends.
- `scrollTop` stands for the drawer's own scroll; `approvePressed` shows Approve plan's hover colour.
- `TicketCard`'s `highlighted` outline is not app UI: it is the amber outline the app's hero
  screenshots draw (`e2e/screenshot/capture-hero.mjs`), here in the warning token.
- `DesignDialog` preview: in the app it is an iframe of the agent's own HTML page, which keeps its
  own light colours in either app theme. The kit draws it light too, with the page's own hex colours
  copied from `e2e/screenshot/design-options.mjs` (`PAGE` in `DesignDialog.tsx`: `#f6f1e9` body,
  `#fffdf9` panel, `#2f6b4f` accent, …), not tokens, since it is not Coppice UI. Only the preview is
  light; the dialog chrome around it (header, tabs, footer, the iframe's border) stays in the dark tokens.
- `DesignDialog` is fixed at the 1280×720 window's `h-[92vh]` (662 px) and `w-[calc(100%-2rem)]`
  (1248 px). The Recommended pill keeps the app's `text-white`.
- "Requested changes" is drawn closed (summary only), as the app's `<details>` starts.
- The metadata column shows only Assignee, Status and the Skip planning switch (the app also has Git actions, Repository,
  Skip human review, Priority, Substatus).
