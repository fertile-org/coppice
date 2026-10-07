import { PLAN_COPY } from '../plan/copy';

export type TicketStatus =
  | 'backlog'
  | 'ready'
  | 'plan_review'
  | 'in_progress'
  | 'in_review'
  | 'in_qa'
  | 'wait_for_final_review'
  | 'done'
  | 'blocked';

export type ColumnColorKey =
  | 'backlog'
  | 'ready'
  | 'plan-review'
  | 'in-progress'
  | 'in-review'
  | 'in-qa'
  | 'wait-final'
  | 'done'
  | 'blocked';

export interface BoardColumnDef {
  status: TicketStatus;
  label: string;
  colorKey: ColumnColorKey;
}

export const BOARD_COLUMNS: BoardColumnDef[] = [
  { status: 'backlog', label: 'Backlog', colorKey: 'backlog' },
  { status: 'ready', label: 'Ready', colorKey: 'ready' },
  { status: 'plan_review', label: PLAN_COPY.column, colorKey: 'plan-review' },
  { status: 'in_progress', label: 'In Progress', colorKey: 'in-progress' },
  { status: 'in_review', label: 'In Review', colorKey: 'in-review' },
  { status: 'in_qa', label: 'In QA', colorKey: 'in-qa' },
  {
    status: 'wait_for_final_review',
    label: 'Wait for Human Review',
    colorKey: 'wait-final',
  },
  { status: 'done', label: 'Done', colorKey: 'done' },
  { status: 'blocked', label: 'Blocked', colorKey: 'blocked' },
];

export const TICKET_STATUSES = new Set<string>(
  BOARD_COLUMNS.map((c) => c.status),
);

export function isTicketStatus(value: string): value is TicketStatus {
  return TICKET_STATUSES.has(value);
}

/** Full Tailwind class names — must be static strings for JIT (no template literals). */
export const COLUMN_COLOR_CLASSES: Record<
  ColumnColorKey,
  { bg: string; border: string; accent: string }
> = {
  backlog: {
    bg: 'bg-column-backlog-bg',
    border: 'border-column-backlog-border',
    accent: 'text-column-backlog-accent',
  },
  ready: {
    bg: 'bg-column-ready-bg',
    border: 'border-column-ready-border',
    accent: 'text-column-ready-accent',
  },
  'plan-review': {
    bg: 'bg-column-plan-review-bg',
    border: 'border-column-plan-review-border',
    accent: 'text-column-plan-review-accent',
  },
  'in-progress': {
    bg: 'bg-column-in-progress-bg',
    border: 'border-column-in-progress-border',
    accent: 'text-column-in-progress-accent',
  },
  'in-review': {
    bg: 'bg-column-in-review-bg',
    border: 'border-column-in-review-border',
    accent: 'text-column-in-review-accent',
  },
  'in-qa': {
    bg: 'bg-column-in-qa-bg',
    border: 'border-column-in-qa-border',
    accent: 'text-column-in-qa-accent',
  },
  'wait-final': {
    bg: 'bg-column-wait-final-bg',
    border: 'border-column-wait-final-border',
    accent: 'text-column-wait-final-accent',
  },
  done: {
    bg: 'bg-column-done-bg',
    border: 'border-column-done-border',
    accent: 'text-column-done-accent',
  },
  blocked: {
    bg: 'bg-column-blocked-bg',
    border: 'border-column-blocked-border',
    accent: 'text-column-blocked-accent',
  },
};

export function columnBgClass(colorKey: ColumnColorKey): string {
  return COLUMN_COLOR_CLASSES[colorKey].bg;
}

export function columnBorderClass(colorKey: ColumnColorKey): string {
  return COLUMN_COLOR_CLASSES[colorKey].border;
}

export function columnAccentClass(colorKey: ColumnColorKey): string {
  return COLUMN_COLOR_CLASSES[colorKey].accent;
}
