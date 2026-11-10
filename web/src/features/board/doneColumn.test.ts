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
  it('orders by Date.parse of updatedAt, newest first, not string order', () => {
    const olderOffset = {
      id: 'b',
      updatedAt: '2026-10-15T12:30:00+01:00',
    };
    const newerUtc = {
      id: 'a',
      updatedAt: '2026-10-15T12:00:00.000Z',
    };

    expect(olderOffset.updatedAt > newerUtc.updatedAt).toBe(true);
    expect(sortDoneTickets([olderOffset, newerUtc]).map((ticket) => ticket.id)).toEqual([
      'a',
      'b',
    ]);
  });

  it('breaks ties by id so the order is stable', () => {
    const first = { id: 'aaa', updatedAt: '2026-10-15T12:00:00Z' };
    const second = { id: 'bbb', updatedAt: '2026-10-15T12:00:00.000Z' };

    expect(sortDoneTickets([second, first]).map((ticket) => ticket.id)).toEqual([
      'aaa',
      'bbb',
    ]);
  });
});

describe('visibleDoneTickets', () => {
  const tickets = Array.from({ length: 32 }, (_, index) => ({
    id: `t${String(index).padStart(2, '0')}`,
  }));

  it('keeps the first page when nothing is focused', () => {
    expect(visibleDoneTickets(tickets, DONE_COLUMN_PAGE_SIZE, -1)).toHaveLength(10);
    expect(visibleDoneTickets(tickets, DONE_COLUMN_PAGE_SIZE, -1)[0]?.id).toBe('t00');
  });

  it('keeps a focused card past the page limit mounted', () => {
    const visible = visibleDoneTickets(tickets, DONE_COLUMN_PAGE_SIZE, 25);
    expect(visible).toHaveLength(26);
    expect(visible.at(-1)?.id).toBe('t25');
  });

  it('does not extend past the list when the page limit is larger', () => {
    expect(visibleDoneTickets(tickets, 40, -1)).toHaveLength(32);
  });
});

function withState(gitState?: string): Ticket {
  return {
    id: gitState ?? 'none',
    humanReview: gitState ? { gitState } : undefined,
  } as unknown as Ticket;
}

describe('needsAction', () => {
  it.each([
    ['not_merged', true],
    ['conflict', true],
    ['main_dirty', true],
    ['changed_after_review', true],
    ['not_pushed', true],
    ['worktree_kept', true],
    ['in_review', false],
    ['merged', false],
  ])('%s -> %s', (state, expected) => {
    expect(needsAction(withState(state))).toBe(expected);
  });

  it('is false without a human review', () => {
    expect(needsAction(withState())).toBe(false);
  });

  it('filterNeedsAction keeps only flagged tickets', () => {
    const list = [withState('merged'), withState('conflict'), withState()];
    expect(filterNeedsAction(list).map((t) => t.id)).toEqual(['conflict']);
  });
});
