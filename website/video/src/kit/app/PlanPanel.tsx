import { ChevronsUpDown, Paperclip } from "lucide-react";
import React from "react";
import { Badge } from "./Badge";
import { Button } from "./Button";
import { DESIGN, DRAWER, PLAN, PLAN_MARKDOWN, type ColumnName } from "./copy";
import { Markdown } from "./Markdown";

// The app has no separate plan panel: the plan is reviewed in the ticket drawer.
// This mirrors web/src/features/tickets/TicketDrawer.tsx (sheet, header, tabs),
// TicketDetailPanel.tsx (title, description, Attach files),
// web/src/features/plan/PlanReviewActions.tsx (Proposed plan, Approve plan) and
// the Assignee/Status fields of TicketMetadataPanel.tsx.
// `design` adds web/src/features/designs/DesignPlanCard.tsx between the description
// and the plan, where TicketDetailPanel.tsx renders it.
//
// After approval the app moves the ticket to Ready, drops the review section and
// writes the plan into the description under "## Approved plan"
// (server/src/copy/plan.rs approved_plan_section_body: the text after "## Plan").
//
// `status` draws a later status than Ready (e.g. In Progress after a code review sends the
// ticket back); `activeRun` draws TicketDrawer's header while a run is active: Run Agent
// and Archive disabled, Stop shown.
//
// Render inside <AppShell>: the drawer covers the whole window like the app's sheet.

// PlanReviewActions' own button classes (not ui/button). Transitions dropped.
const PRIMARY = "rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground";
const PRIMARY_PRESSED = "rounded-md bg-accent-hover px-3 py-1.5 font-body text-sm font-medium text-accent-foreground";
const SECONDARY = "rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-primary";

const FIELD = {
  borderColor: "var(--color-field-border)",
  backgroundColor: "var(--color-field-bg)",
  boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.06)",
} as const;

const TABS = ["Detail", "Live Console", "Agent Runs"];

const afterPlanHeading = (lines: string[]) => {
  const at = lines.findIndex((l) => l.trim() === PLAN_MARKDOWN.planHeading);
  return at === -1 ? lines : lines.slice(at + 1);
};

export const PlanPanel: React.FC<{
  title: string;
  /** Plan markdown, one line per entry, as the PM writes it ("## Plan", "### Approach", …). */
  planMarkdownLines: string[];
  /** How many plan lines are on screen. 0 hides the review section (no plan yet). */
  revealedLines: number;
  approved: boolean;
  /** The ticket description, one markdown line per entry. */
  description?: string[];
  /** Assignee field, shown as "name (role)" like the app's combobox. */
  assignee?: { name: string; role: string };
  revision?: number;
  /** Approve plan under the cursor at the moment of the click. */
  approvePressed?: boolean;
  /** Scroll offset of the Detail column in logical px (the app scrolls it to reach Approve plan). */
  scrollTop?: number;
  /** The plan's design set: option count, and the option work follows once it drives the work. */
  design?: { count: number; using?: { label: string; title: string; yourChoice: boolean } };
  /** "Open" on the design card under the cursor at the moment of the click. */
  designOpenPressed?: boolean;
  /** A plan run is queued or running: Approve plan is disabled (opacity 50). */
  planRunActive?: boolean;
  /** Earlier change requests, shown as the collapsed "Requested changes" box. */
  changeRequests?: string[];
  /** Status shown in the header badge and Status field; defaults to Plan Review, or Ready once approved. */
  status?: ColumnName;
  /** An agent run is active on the ticket. */
  activeRun?: boolean;
}> = ({
  title,
  planMarkdownLines,
  revealedLines,
  approved,
  description = [],
  assignee,
  revision = 1,
  approvePressed = false,
  scrollTop = 0,
  design,
  designOpenPressed = false,
  planRunActive = false,
  changeRequests = [],
  status,
  activeRun = false,
}) => {
  const shownStatus: ColumnName = status ?? (approved ? "Ready" : "Plan Review");
  const descriptionLines = approved
    ? [...description, PLAN_MARKDOWN.approvedPlanSection, ...afterPlanHeading(planMarkdownLines)]
    : description;
  const shown = planMarkdownLines.slice(0, Math.max(0, revealedLines));

  return (
    <div className="absolute inset-0 z-50">
      <div className="absolute inset-0 bg-overlay backdrop-blur-[1px]" />
      <div className="absolute inset-y-0 right-0 flex h-full w-[90%] flex-col bg-surface-raised shadow-2xl">
        <header className="flex shrink-0 items-end justify-between gap-4 border-b border-border px-6 py-4">
          <div className="min-w-0">
            <p className="font-body text-xs uppercase tracking-wide text-text-muted">Ticket</p>
            <h2 className="truncate font-display text-xl font-semibold text-text-primary">{title}</h2>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <Badge status={shownStatus} />
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button padding="px-3 py-1.5" disabled={activeRun}>
              Run Agent
            </Button>
            {activeRun && <Button variant="destructive">{DRAWER.stop}</Button>}
            <Button variant="secondary" disabled={activeRun}>
              Archive
            </Button>
            <Button variant="secondary">Close</Button>
          </div>
        </header>

        <nav className="flex shrink-0 gap-1 border-b border-border px-6">
          {TABS.map((tab) => (
            <div
              key={tab}
              className={`border-b-2 px-4 py-3 font-body text-sm ${
                tab === "Detail" ? "border-accent text-accent" : "border-transparent text-text-secondary"
              }`}
            >
              {tab}
            </div>
          ))}
        </nav>

        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(0,calc(16rem+50px))] overflow-hidden">
          <div className="min-h-0 overflow-hidden px-6 py-5">
            <div className="flex flex-col gap-8" style={{ translate: `0px ${-scrollTop}px` }}>
              <div className="flex flex-col gap-4">
                <div className="flex items-start justify-between gap-4">
                  <h3 className="font-display text-lg font-semibold text-text-primary">{title}</h3>
                  <div className="flex shrink-0 gap-2">
                    <Button variant="secondary">Edit</Button>
                  </div>
                </div>
                <div>
                  <div className="min-h-[200px] rounded-md border border-border bg-surface px-4 py-3">
                    {descriptionLines.length > 0 ? (
                      <Markdown lines={descriptionLines} />
                    ) : (
                      <p className="font-body text-sm italic text-text-muted">No description yet.</p>
                    )}
                  </div>
                  <div className="mt-2 flex">
                    <Button variant="secondary" size="sm">
                      <Paperclip className="size-4" aria-hidden="true" />
                      Attach files
                    </Button>
                  </div>
                </div>
              </div>

              {design && (
                <section className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-accent bg-surface-raised px-4 py-3">
                  <div>
                    <h3 className="font-body text-sm font-medium text-text-primary">{DESIGN.cardTitle}</h3>
                    <p className="font-body text-xs text-text-secondary">
                      {design.using
                        ? `${DESIGN.options(design.count)} · ${DESIGN.using(design.using, design.using.yourChoice)}`
                        : DESIGN.options(design.count)}
                    </p>
                  </div>
                  <Button size="sm" pressed={designOpenPressed}>
                    {DESIGN.open}
                  </Button>
                </section>
              )}

              {!approved && shown.length > 0 && (
                <section className="space-y-3 border-t border-border pt-6">
                  <div className="flex flex-wrap items-baseline gap-2">
                    <h3 className="font-display text-base font-semibold tracking-tight text-text-primary">
                      {PLAN.proposedPlan}
                    </h3>
                    <p className="font-body text-xs text-text-secondary">{PLAN.revision(revision)}</p>
                  </div>
                  <div className="rounded-md border border-accent/40 bg-surface px-4 py-3">
                    <Markdown lines={shown} />
                  </div>
                  {changeRequests.length > 0 && (
                    // A closed <details>: only the summary shows, with the browser's marker.
                    <div className="rounded-md border border-border bg-surface px-3 py-2">
                      <p className="font-body text-sm text-text-primary">
                        <span className="mr-1.5 text-xs">▸</span>
                        {PLAN.requestedChanges}
                      </p>
                    </div>
                  )}
                  <p className="font-body text-xs text-text-secondary">{PLAN.approveAddsToDescription}</p>
                  <div className="flex flex-col gap-2">
                    {/* rows={3}; placeholder is the preflight #9ca3af in the app, nearest token text-secondary. */}
                    <div className="h-[78px] w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-secondary">
                      {PLAN.askPlaceholder}
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      {/* Disabled while the note is empty: disabled:opacity-50. */}
                      <div className={SECONDARY} style={{ opacity: 0.5 }}>
                        {PLAN.askForChanges}
                      </div>
                      <div className="ml-auto flex flex-wrap items-center gap-2">
                        <div className="flex items-center gap-2 font-body text-xs text-text-secondary">
                          <input type="checkbox" checked={false} readOnly className="h-3.5 w-3.5" />
                          {PLAN.skipHumanReview}
                        </div>
                        <div
                          className={approvePressed ? PRIMARY_PRESSED : PRIMARY}
                          style={planRunActive ? { opacity: 0.5 } : undefined}
                        >
                          {PLAN.approvePlan}
                        </div>
                      </div>
                    </div>
                  </div>
                </section>
              )}
            </div>
          </div>

          <div className="min-h-0 overflow-hidden border-l border-border px-4 py-5">
            <div className="space-y-5">
              {assignee && (
                <div className="space-y-2">
                  <p className="font-body text-sm font-medium leading-none text-text-secondary">Assignee</p>
                  <div
                    className="flex h-10 w-full items-center justify-between gap-2 rounded-md border px-3 py-2 font-body text-sm text-text-primary"
                    style={FIELD}
                  >
                    <span className="truncate">{`${assignee.name} (${assignee.role})`}</span>
                    <ChevronsUpDown className="size-4 shrink-0 text-text-muted" aria-hidden />
                  </div>
                  <p className="font-body text-xs text-text-muted">Assigned to {assignee.name}</p>
                </div>
              )}
              <div className="space-y-2">
                <p className="font-body text-sm font-medium leading-none text-text-secondary">Status</p>
                <div
                  className="flex min-h-10 w-full items-center justify-between gap-2 rounded-md border px-3 py-2 font-body text-sm"
                  style={FIELD}
                >
                  <Badge status={shownStatus} />
                  <ChevronsUpDown className="size-4 shrink-0 text-text-muted" aria-hidden />
                </div>
              </div>
              {/* Skip planning switch row (TicketMetadataPanel.tsx, ui/switch.tsx, unchecked). */}
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="font-body text-sm font-medium leading-none text-text-secondary">{PLAN.skipPlanning}</p>
                  <p className="mt-0.5 font-body text-xs text-text-muted">{PLAN.skipPlanningHelp}</p>
                </div>
                <div className="mt-0.5 inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-transparent bg-switch-track-off">
                  <div className="block size-4 rounded-full bg-surface-raised shadow-sm" style={{ translate: "2px 0px" }} />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
