export type TicketStatus =
  | 'backlog'
  | 'ready'
  | 'in_progress'
  | 'in_review'
  | 'in_qa'
  | 'wait_for_final_review'
  | 'done'
  | 'blocked';

export type ColumnColorKey =
  | 'backlog'
  | 'ready'
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
  { status: 'in_progress', label: 'In Progress', colorKey: 'in-progress' },
  { status: 'in_review', label: 'In Review', colorKey: 'in-review' },
  { status: 'in_qa', label: 'In QA', colorKey: 'in-qa' },
  {
    status: 'wait_for_final_review',
    label: 'Wait for Final Review',
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

export function columnBgClass(colorKey: ColumnColorKey): string {
  return `bg-column-${colorKey}-bg`;
}

export function columnBorderClass(colorKey: ColumnColorKey): string {
  return `border-column-${colorKey}-border`;
}

export function columnAccentClass(colorKey: ColumnColorKey): string {
  return `text-column-${colorKey}-accent`;
}
