import type { Ticket } from './useTickets';

export const DONE_COLUMN_PAGE_SIZE = 10;

function updatedAtMs(value: string): number {
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? 0 : parsed;
}

/** Newest Done first. Compare instants, not timestamp strings. */
export function sortDoneTickets<T extends { id: string; updatedAt: string }>(
  tickets: readonly T[],
): T[] {
  return [...tickets].sort((left, right) => {
    const byTime = updatedAtMs(right.updatedAt) - updatedAtMs(left.updatedAt);
    if (byTime !== 0) return byTime;
    if (left.id < right.id) return -1;
    if (left.id > right.id) return 1;
    return 0;
  });
}

export function visibleDoneCount(
  total: number,
  pageLimit: number,
  focusedIndex: number,
): number {
  const keepFocused = focusedIndex >= 0 ? focusedIndex + 1 : 0;
  return Math.min(total, Math.max(pageLimit, keepFocused));
}

/** First page, or through a focused card that would otherwise unmount. */
export function visibleDoneTickets<T>(
  sorted: readonly T[],
  pageLimit: number,
  focusedIndex: number,
): T[] {
  return sorted.slice(0, visibleDoneCount(sorted.length, pageLimit, focusedIndex));
}

/** Mirrors the server's `needs_action()`: every git state except in_review and merged. */
export function needsAction(ticket: Ticket): boolean {
  const state = ticket.humanReview?.gitState;
  return state != null && state !== 'in_review' && state !== 'merged';
}

export function filterNeedsAction<T extends Ticket>(tickets: readonly T[]): T[] {
  return tickets.filter(needsAction);
}
