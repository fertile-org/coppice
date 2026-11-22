import { describe, expect, it } from 'vitest';
import type { Ticket } from './useTickets';
import {
  DONE_COLUMN_PAGE_SIZE,
  filterNeedsAction,
  needsAction,
  sortDoneTickets,
  visibleDoneTickets,
} from './doneColumn';

describe('sortDoneTickets', () => {
  it('orders by Date.parse (newest first) and breaks ties by id', () => {
    const olderOffset = { id: 'b', updatedAt: '2026-10-15T12:30:00+01:00' };
    const newerUtc = { id: 'a', updatedAt: '2026-10-15T12:00:00.000Z' };
    expect(olderOffset.updatedAt > newerUtc.updatedAt).toBe(true);
    expect(sortDoneTickets([olderOffset, newerUtc]).map((t) => t.id)).toEqual(['a', 'b']);

    const first = { id: 'aaa', updatedAt: '2026-10-15T12:00:00Z' };
    const second = { id: 'bbb', updatedAt: '2026-10-15T12:00:00.000Z' };
    expect(sortDoneTickets([second, first]).map((t) => t.id)).toEqual(['aaa', 'bbb']);
  });
});

describe('visibleDoneTickets', () => {
  const tickets = Array.from({ length: 32 }, (_, index) => ({
    id: `t${String(index).padStart(2, '0')}`,
  }));

  it('pages the Done column and expands for a focused ticket', () => {
    const cases = [
      [DONE_COLUMN_PAGE_SIZE, -1, 10],
      [DONE_COLUMN_PAGE_SIZE, 25, 26],
      [40, -1, 32],
    ] as const;
    for (const [pageSize, focusIndex, length] of cases) {
      const visible = visibleDoneTickets(tickets, pageSize, focusIndex);
      expect(visible).toHaveLength(length);
      if (focusIndex >= 0) {
        expect(visible.at(-1)?.id).toBe(`t${String(focusIndex).padStart(2, '0')}`);
      } else {
        expect(visible[0]?.id).toBe('t00');
      }
    }
  });
});

function withState(gitState?: string): Ticket {
  return {
    id: gitState ?? 'none',
    humanReview: gitState ? { gitState } : undefined,
  } as unknown as Ticket;
}

describe('needsAction', () => {
  it('flags git states that need action', () => {
    const needs = [
      'not_merged',
      'conflict',
      'main_dirty',
      'changed_after_review',
      'not_pushed',
      'worktree_kept',
    ] as const;
    for (const state of needs) {
      expect(needsAction(withState(state))).toBe(true);
    }
    expect(needsAction(withState('merged'))).toBe(false);
    expect(needsAction(withState())).toBe(false);
  });

  it('filterNeedsAction keeps only flagged tickets', () => {
    const list = [withState('merged'), withState('conflict'), withState()];
    expect(filterNeedsAction(list).map((t) => t.id)).toEqual(['conflict']);
  });
});
