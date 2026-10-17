import type { BoardColumnSnapshot, BoardFocus } from './boardFocus';
import { BOARD_COLUMNS, type TicketStatus } from './columns';

export function adjacentColumn(status: TicketStatus, direction: 1 | -1) {
  const index = BOARD_COLUMNS.findIndex((column) => column.status === status);
  if (index < 0) return null;
  return BOARD_COLUMNS[index + direction] ?? null;
}

export function columnIndexForFocus(
  columns: readonly BoardColumnSnapshot[],
  focus: BoardFocus,
): number | null {
  if (focus.type === 'heading') {
    return focus.column >= 0 && focus.column < columns.length ? focus.column : null;
  }
  const index = columns.findIndex((column) => column.ticketIds.includes(focus.ticketId));
  return index >= 0 ? index : null;
}

export function addTicketField(columnIndex: number): HTMLInputElement | null {
  const node = document.querySelector(
    `input[data-add-ticket][data-column-index="${columnIndex}"]`,
  );
  if (!(node instanceof HTMLInputElement) || node.disabled) return null;
  return node;
}

export function moveTargetStatus(
  ticket: { status: TicketStatus; archivedAt?: string | null } | null | undefined,
  direction: 1 | -1,
): TicketStatus | null {
  if (!ticket || ticket.archivedAt) return null;
  return adjacentColumn(ticket.status, direction)?.status ?? null;
}

export function approveShortcutEnabled(
  ticket:
    | {
        status: string;
        archivedAt?: string | null;
        hasPlan?: boolean;
        planQuestions?: { waiting: boolean } | null;
      }
    | null
    | undefined,
): boolean {
  return (
    ticket?.status === 'plan_review' &&
    !ticket.archivedAt &&
    ticket.hasPlan !== false &&
    !ticket.planQuestions?.waiting
  );
}

export function acceptShortcutEnabled(
  ticket: { status: string; archivedAt?: string | null } | null | undefined,
): boolean {
  return ticket?.status === 'wait_for_final_review' && !ticket.archivedAt;
}

export function codeReviewEnabled(
  ticket: { repoId?: string } | null | undefined,
  repos: readonly { id: string; verificationStatus: string }[] | undefined,
): boolean {
  if (!ticket?.repoId) return false;
  const repo = repos?.find((item) => item.id === ticket.repoId);
  if (repo && repo.verificationStatus !== 'ready') return false;
  return true;
}
