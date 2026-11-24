import React from "react";
import { Button } from "./Button";
import { ComboboxList, ComboboxTrigger, FIELD, FIELD_FOCUSED } from "./Combobox";
import {
  SUBMIT_REVIEW,
  THEME_TOGGLE_REVIEW,
  formatReviewPreview,
  type InlineComment,
  type WorkflowActionLabel,
} from "./copy";

// Mirrors web/src/features/code/SubmitReviewDialog.tsx for a review linked to a ticket,
// in web/src/components/ui/dialog.tsx (DialogContent, no close button):
// z-[60] overlay, max-w-2xl, max-h-[90vh] of the 720 px window (648 px), p-0.
// Header (Submit review, "Post a combined review comment on the linked ticket."),
// Summary (ui/textarea.tsx rows={4}), Workflow action (ui/combobox.tsx; three options, so
// no search box), Engineer (shown for Move to In Progress, prefilled with the ticket's
// assignee when it is an engineer), Preview (formatReviewPreview), footer Cancel / Submit review.
// The open Workflow action list is the combobox's popover (ui/popover.tsx, ui/command.tsx):
// the highlighted item has bg-accent-muted, the current value a Check.
// `enter` (0 → 1) draws the dialog-in / overlay-in motion (opacity, 0.5rem rise).
//
// Render as a child of <CodeReview>.

const ACTIONS: WorkflowActionLabel[] = [
  SUBMIT_REVIEW.commentOnly,
  SUBMIT_REVIEW.moveToInProgress,
  SUBMIT_REVIEW.reassignEngineer,
];

const Label: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p className="font-body text-sm font-medium leading-none text-text-secondary">{children}</p>
);

export const SubmitReviewDialog: React.FC<{
  /** What the Summary box shows (a TypeText while typing); empty shows the placeholder. */
  summary?: React.ReactNode;
  /** The Summary text so far, for the Preview. */
  summaryText: string;
  summaryFocused?: boolean;
  action: WorkflowActionLabel;
  /** The Workflow action list is open, with this item highlighted. */
  menuHighlight?: WorkflowActionLabel;
  /** The prefilled engineer, shown once the action needs one. */
  engineer: { name: string; role: string };
  inlineComments: InlineComment[];
  submitPressed?: boolean;
  /** The review is being posted (Button loading). */
  submitting?: boolean;
  enter?: number;
}> = ({
  summary,
  summaryText,
  summaryFocused = false,
  action,
  menuHighlight,
  engineer,
  inlineComments,
  submitPressed = false,
  submitting = false,
  enter = 1,
}) => {
  const r = THEME_TOGGLE_REVIEW;
  const preview = formatReviewPreview(
    r.repoName,
    r.worktreePath,
    r.baseBranch,
    r.headBranch,
    r.headSha,
    summaryText.trim() || SUBMIT_REVIEW.summaryRequired,
    inlineComments,
  );
  const needsEngineer = action !== SUBMIT_REVIEW.commentOnly;
  return (
    <div className="absolute inset-0 z-[60]">
      <div className="absolute inset-0 bg-overlay" style={{ opacity: enter }} />
      <div
        className="absolute inset-0 m-auto flex h-fit max-h-[648px] w-[672px] flex-col overflow-hidden rounded-xl border border-border bg-paper-50 p-0 shadow-lg"
        style={{ opacity: enter, translate: `0px ${(1 - enter) * 8}px` }}
      >
        <div className="border-b border-border px-6 py-4">
          <h2 className="font-display text-xl font-semibold text-bark-900">{SUBMIT_REVIEW.title}</h2>
          <p className="mt-1 font-body text-sm text-text-secondary">{SUBMIT_REVIEW.descriptionLinked}</p>
        </div>

        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="min-h-0 flex-1 space-y-4 overflow-hidden px-6 py-4">
            <div className="space-y-2">
              <Label>{SUBMIT_REVIEW.summary}</Label>
              {/* rows={4}: four 20 px lines plus py-2 and the border. */}
              <div
                className="h-[98px] w-full rounded-md border px-3 py-2 font-body text-sm text-text-primary"
                style={summaryFocused ? FIELD_FOCUSED : FIELD}
              >
                {summaryText ? summary : <span className="text-text-muted">{SUBMIT_REVIEW.summaryPlaceholder}</span>}
              </div>
            </div>

            <div className="relative z-10 space-y-2">
              <Label>{SUBMIT_REVIEW.workflowAction}</Label>
              <ComboboxTrigger label={action} />
              {menuHighlight !== undefined && (
                <ComboboxList options={ACTIONS} value={action} highlight={menuHighlight} />
              )}
            </div>

            {needsEngineer && (
              <div className="space-y-2">
                <Label>{SUBMIT_REVIEW.engineer}</Label>
                <ComboboxTrigger label={SUBMIT_REVIEW.engineerOption(engineer.name, engineer.role)} />
              </div>
            )}

            <div className="space-y-2">
              <Label>{SUBMIT_REVIEW.preview}</Label>
              <pre className="max-h-48 overflow-hidden whitespace-pre-wrap rounded-md border border-border bg-surface px-3 py-2 font-mono text-xs text-text-secondary">
                {preview}
              </pre>
            </div>
          </div>

          <div className="flex justify-end gap-2 border-t border-border px-6 py-4">
            <Button variant="secondary">{SUBMIT_REVIEW.cancel}</Button>
            <Button pressed={submitPressed} loading={submitting}>
              {SUBMIT_REVIEW.submit}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
};
