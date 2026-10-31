import { describe, expect, it } from 'vitest';
import {
  DONE_COLUMN_PAGE_SIZE,
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
