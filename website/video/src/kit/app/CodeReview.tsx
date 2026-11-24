import { ArrowLeft, ChevronsUpDown } from "lucide-react";
import React from "react";
import { AbsoluteFill } from "remotion";
import "../tokens";
import { APP_LOGICAL } from "./AppShell";
import { Button } from "./Button";
import { APPROVED_PLAN, CODE_REVIEW, THEME_TOGGLE_REVIEW, worktreeLabel, type CodeToken } from "./copy";

// The code review page, web/src/features/code/CodeReviewPage.tsx, for a ticket in
// Wait for Human Review with an approved plan (three columns), Unified view:
// - header: CodeReviewBackButton.tsx (ghost Back), "Code review", the ticket title,
//   MergeLine (repo · head branch chip, sha, into, base chip, sha), Submit review;
//   the Worktree / Base selects (ui/combobox.tsx triggers), Split / Unified, Hide whitespace.
// - web/src/features/plan/PlanChecklist.tsx (Approved plan, steps only: no criteria).
// - ChangedFilesPanel.tsx (Changed files, "0 of 1 viewed", one row).
// - FileReviewHeader.tsx (Viewed, path, +/−, 1 / 1, Prev / Next disabled).
// - DiffViewer.tsx: react-diff-view's unified table with the app's .code-review-diff colours
//   (web/src/index.css) and highlight.js token colours; InlineCommentWidget (compose) and
//   PendingCommentBubble (posted) in the widget row under the line.
// Not drawn: ReviewerBriefBar.tsx (shown only when the agent left a reviewer brief for this
// commit; the app's own screenshot fixture has none), the grain texture.
//
// The page is outside AppShell in the app (route /code), so it fills the whole window.
// Render the SubmitReviewDialog as a child: it covers the page like the app's dialog.

type Review = typeof THEME_TOGGLE_REVIEW;

const TOKEN_CLASS: Record<NonNullable<CodeToken["kind"]>, string> = {
  keyword: "text-danger", // .hljs-keyword
  string: "text-success", // .hljs-string
  title: "text-info", // .hljs-title, .hljs-name
  attr: "text-accent", // .hljs-attr
};

const FIELD = {
  borderColor: "var(--color-field-border)",
  backgroundColor: "var(--color-field-bg)",
  boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.06)",
} as const;

/** .field-control:focus-visible */
const FIELD_FOCUSED = {
  borderColor: "var(--color-accent)",
  backgroundColor: "var(--color-field-bg)",
  boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.04), 0 0 0 2px var(--color-accent-muted)",
} as const;

const Select: React.FC<{ label: string; className: string }> = ({ label, className }) => (
  <div
    className={`flex h-10 items-center justify-between gap-2 rounded-md border px-3 py-2 font-body text-sm text-text-primary ${className}`}
    style={FIELD}
  >
    <span className="min-w-0 truncate">{label}</span>
    <ChevronsUpDown className="size-4 shrink-0 text-text-muted" aria-hidden />
  </div>
);

const BranchChip: React.FC<{ name: string }> = ({ name }) => (
  <span className="inline-block max-w-[12rem] min-w-0 shrink truncate rounded bg-accent-muted px-1.5 py-px font-mono text-xs text-accent">
    {name}
  </span>
);

const Checkbox: React.FC = () => (
  <input type="checkbox" checked={false} readOnly disabled className="mt-0.5" />
);

export type DiffComment = {
  /** New-side line number the comment is on. */
  line: number;
  /** compose: InlineCommentWidget (new); posted: PendingCommentBubble. */
  mode: "compose" | "posted";
  /** What the textarea or bubble shows (a TypeText while typing). */
  body: React.ReactNode;
  /** The draft has text: Add comment is enabled. */
  hasText: boolean;
  addPressed?: boolean;
};

const CommentWidget: React.FC<{ comment: DiffComment }> = ({ comment }) =>
  comment.mode === "compose" ? (
    <div className="rounded-md border border-border bg-paper-50 p-3 shadow-sm">
      {/* Textarea rows={3}, autoFocus: focused field-control. */}
      <div
        className="min-h-[78px] w-full rounded-md border px-3 py-2 font-body text-sm text-text-primary"
        style={FIELD_FOCUSED}
      >
        {comment.hasText ? comment.body : <span className="text-text-muted">{CODE_REVIEW.leaveComment}</span>}
      </div>
      <div className="mt-2 flex items-center gap-2">
        <Button size="sm" disabled={!comment.hasText} pressed={comment.addPressed}>
          {CODE_REVIEW.addComment}
        </Button>
        <Button variant="ghost" size="sm">
          {CODE_REVIEW.cancel}
        </Button>
      </div>
    </div>
  ) : (
    <div className="rounded-md border border-accent-muted bg-accent-muted/30 p-3">
      <p className="whitespace-pre-wrap font-body text-sm text-text-primary">{comment.body}</p>
      <div className="mt-2 flex gap-2">
        <Button variant="ghost" size="sm">
          {CODE_REVIEW.edit}
        </Button>
        <Button variant="ghost" size="sm">
          {CODE_REVIEW.remove}
        </Button>
      </div>
    </div>
  );

const GUTTER = "px-[1ch] text-right";

const DiffTable: React.FC<{ review: Review; comment?: DiffComment }> = ({ review, comment }) => (
  <div className="min-h-0 flex-1 overflow-hidden bg-background p-4">
    <div className="overflow-hidden rounded-lg border border-border bg-paper-50">
      <table className="w-full table-fixed border-collapse font-mono text-sm leading-[1.5] text-text-primary">
        <colgroup>
          <col style={{ width: "7ch" }} />
          <col style={{ width: "7ch" }} />
          <col />
        </colgroup>
        <tbody>
          {review.rows.map((row) => {
            const gutter = row.insert ? "bg-success-muted text-text-secondary" : "";
            return (
              <React.Fragment key={row.newLine}>
                <tr>
                  <td className={`${GUTTER} ${gutter}`}>{row.oldLine ?? ""}</td>
                  <td className={`${GUTTER} ${gutter}`}>{row.newLine}</td>
                  <td className={`whitespace-pre-wrap break-all pl-[0.5em] ${row.insert ? "bg-success-muted" : ""}`}>
                    {row.tokens.map((t, i) => (
                      <span key={i} className={t.kind ? TOKEN_CLASS[t.kind] : undefined}>
                        {t.text}
                      </span>
                    ))}
                  </td>
                </tr>
                {comment?.line === row.newLine && (
                  <tr>
                    <td colSpan={3} className="font-body">
                      <CommentWidget comment={comment} />
                    </td>
                  </tr>
                )}
              </React.Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
    <p className="mt-3 font-body text-xs text-text-muted">{CODE_REVIEW.clickLineNumber}</p>
  </div>
);

export const CodeReview: React.FC<{
  review?: Review;
  comment?: DiffComment;
  /** Submit review (header) under the cursor at the moment of the click. */
  submitPressed?: boolean;
  /** The dialog, drawn over the page. */
  children?: React.ReactNode;
}> = ({ review = THEME_TOGGLE_REVIEW, comment, submitPressed = false, children }) => {
  const slash = review.path.lastIndexOf("/");
  const dir = review.path.slice(0, slash + 1);
  const name = review.path.slice(slash + 1);
  return (
    <AbsoluteFill className="bg-background">
      <div
        className="relative flex flex-col overflow-hidden bg-background"
        style={{
          width: APP_LOGICAL.width,
          height: APP_LOGICAL.height,
          scale: String(APP_LOGICAL.scale),
          transformOrigin: "0 0",
        }}
      >
        <header className="shrink-0 border-b border-border bg-surface px-5 pt-3">
          <div className="flex flex-nowrap items-start gap-4">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <Button variant="ghost">
                  <ArrowLeft className="size-4" aria-hidden="true" />
                  {CODE_REVIEW.back}
                </Button>
                <span className="font-body text-xs text-text-muted">{CODE_REVIEW.codeReview}</span>
              </div>
              <h1 className="mt-1 truncate font-display text-xl font-semibold text-text-primary">{review.ticketTitle}</h1>
              <p className="mt-0.5 flex min-w-0 flex-nowrap items-center gap-1.5 overflow-hidden font-body text-sm text-text-secondary">
                <span className="max-w-[12rem] min-w-0 shrink truncate">{review.repoName}</span>
                <span className="shrink-0 text-text-muted">·</span>
                <BranchChip name={review.headBranch} />
                <span className="shrink-0 font-mono text-xs text-text-muted">{review.headSha.slice(0, 7)}</span>
                <span className="shrink-0">{CODE_REVIEW.into}</span>
                <BranchChip name={review.baseBranch} />
                <span className="shrink-0 font-mono text-xs text-text-muted">{review.baseSha.slice(0, 7)}</span>
              </p>
            </div>
            <div className="mt-5 shrink-0">
              <Button pressed={submitPressed}>{CODE_REVIEW.submitReview}</Button>
            </div>
          </div>

          <div className="mt-2.5 flex h-12 flex-nowrap items-center gap-3 border-t border-border">
            <span className="shrink-0 font-body text-xs text-text-muted">{CODE_REVIEW.worktree}</span>
            <Select
              label={worktreeLabel(review.worktreePath, review.headBranch, review.ticketTitle)}
              className="w-72 min-w-0 shrink"
            />
            <span className="shrink-0 font-body text-xs text-text-muted">{CODE_REVIEW.base}</span>
            <Select label={review.baseBranch} className="w-40 shrink-0" />
            <div className="ml-auto flex shrink-0 items-center gap-3">
              <div className="flex rounded-md border border-border bg-surface-raised p-0.5">
                <span className="rounded px-2.5 py-1 font-body text-xs text-text-secondary">{CODE_REVIEW.split}</span>
                <span className="rounded bg-accent-muted px-2.5 py-1 font-body text-xs text-accent">
                  {CODE_REVIEW.unified}
                </span>
              </div>
              <label className="flex items-center gap-1.5 whitespace-nowrap font-body text-xs text-text-secondary">
                <input type="checkbox" checked={false} readOnly className="accent-accent" />
                {CODE_REVIEW.hideWhitespace}
              </label>
            </div>
          </div>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-[240px_280px_1fr]">
          <aside className="min-h-0 overflow-hidden border-r border-border bg-surface px-4 py-4">
            <h2 className="font-display text-sm font-semibold text-text-primary">{APPROVED_PLAN}</h2>
            <ul className="mt-3 space-y-2">
              {review.planSteps.map((step) => (
                <li key={step}>
                  <label className="flex items-start gap-2 font-body text-sm text-text-primary">
                    <Checkbox />
                    <span>{step}</span>
                  </label>
                </li>
              ))}
            </ul>
          </aside>

          <div className="flex h-full min-h-0 flex-col border-r border-border bg-surface-raised">
            <div className="shrink-0 border-b border-border px-4 py-3">
              <h2 className="font-body text-sm font-medium text-text-primary">{CODE_REVIEW.changedFiles}</h2>
              <div className="mt-2">
                <p className="font-body text-xs text-text-muted">{CODE_REVIEW.viewedProgress(0, 1)}</p>
                <div className="mt-1 h-1 overflow-hidden rounded-full bg-border" />
              </div>
            </div>
            <ul className="min-h-0 flex-1 py-1">
              {/* The selected row (ChangedFilesPanel.tsx). */}
              <li className="flex w-full items-start gap-2 bg-accent-muted/50 px-3 py-2 text-left text-text-primary">
                <span className="mt-0.5 w-3 shrink-0" />
                <span className="mt-0.5 inline-flex shrink-0 items-center rounded-full border border-border bg-surface px-2.5 py-0.5 font-mono text-xs font-medium text-text-secondary">
                  {review.status}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-mono text-xs font-medium">{name}</span>
                  <span className="mt-0.5 block truncate font-mono text-[11px] text-text-muted">{dir}</span>
                </span>
                <span className="shrink-0 font-mono text-[11px]">
                  <span className="text-success">+{review.additions}</span>{" "}
                  <span className="text-danger">−{review.deletions}</span>
                </span>
              </li>
            </ul>
          </div>

          <div className="flex h-full min-h-0 min-w-0 flex-col">
            <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b border-border bg-surface px-4 py-2">
              <label className="flex items-center gap-2 font-body text-sm text-text-secondary">
                <input type="checkbox" checked={false} readOnly />
                {CODE_REVIEW.viewed}
              </label>
              <p className="flex min-w-0 flex-1 items-baseline font-mono text-sm">
                <span className="min-w-0 truncate text-text-muted">{dir}</span>
                <span className="shrink-0 font-semibold text-text-primary">{name}</span>
              </p>
              <p className="font-mono text-xs">
                <span className="text-success">+{review.additions}</span>{" "}
                <span className="text-danger">−{review.deletions}</span>
              </p>
              <div className="ml-auto flex items-center gap-2">
                <span className="font-mono text-xs text-text-muted">1 / 1</span>
                <Button variant="ghost" size="sm" disabled>
                  {CODE_REVIEW.prev} <span className="font-mono text-[10px] text-text-muted">{CODE_REVIEW.prevKey}</span>
                </Button>
                <Button variant="ghost" size="sm" disabled>
                  {CODE_REVIEW.next} <span className="font-mono text-[10px] text-text-muted">{CODE_REVIEW.nextKey}</span> ›
                </Button>
              </div>
            </div>
            <DiffTable review={review} comment={comment} />
          </div>
        </div>

        {children}
      </div>
    </AbsoluteFill>
  );
};
