import type { ColumnColorKey } from "./copy";

// web/src/features/board/columns.ts (COLUMN_COLOR_CLASSES). Full static class
// names so Tailwind finds them.
const CLASSES: Record<ColumnColorKey, { bg: string; border: string; accent: string }> = {
  backlog: { bg: "bg-column-backlog-bg", border: "border-column-backlog-border", accent: "text-column-backlog-accent" },
  ready: { bg: "bg-column-ready-bg", border: "border-column-ready-border", accent: "text-column-ready-accent" },
  "plan-review": {
    bg: "bg-column-plan-review-bg",
    border: "border-column-plan-review-border",
    accent: "text-column-plan-review-accent",
  },
  "in-progress": {
    bg: "bg-column-in-progress-bg",
    border: "border-column-in-progress-border",
    accent: "text-column-in-progress-accent",
  },
  "in-review": {
    bg: "bg-column-in-review-bg",
    border: "border-column-in-review-border",
    accent: "text-column-in-review-accent",
  },
  "in-qa": { bg: "bg-column-in-qa-bg", border: "border-column-in-qa-border", accent: "text-column-in-qa-accent" },
  "wait-final": {
    bg: "bg-column-wait-final-bg",
    border: "border-column-wait-final-border",
    accent: "text-column-wait-final-accent",
  },
  done: { bg: "bg-column-done-bg", border: "border-column-done-border", accent: "text-column-done-accent" },
};

export const columnClasses = (key: ColumnColorKey) => CLASSES[key];
