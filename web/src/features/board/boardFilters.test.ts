import { describe, expect, it } from 'vitest';
import {
  activityWindow,
  countActiveBoardFilters,
  DEFAULT_BOARD_FILTERS,
  filterTickets,
  includeArchivedForVisibility,
  parseBoardFilters,
  writeBoardFilters,
  type BoardFilters,
} from './boardFilters';
import type { Ticket } from './useTickets';

function ticket(
  overrides: Pick<Ticket, 'id' | 'title' | 'status'> & Partial<Ticket>,
): Ticket {
  return {
    boardId: 'board-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-08-01T00:00:00.000Z',
    updatedAt: '2026-08-01T00:00:00.000Z',
    lastActivityAt: '2026-08-01T00:00:00.000Z',
    ...overrides,
  };
}

describe('parseBoardFilters / writeBoardFilters', () => {
  it('returns defaults for empty params', () => {
    expect(parseBoardFilters(new URLSearchParams())).toEqual(
      DEFAULT_BOARD_FILTERS,
    );
  });

  it('round-trips non-default filters and preserves ticket', () => {
    const filters: BoardFilters = {
      q: 'auth',
      visibility: 'include_archived',
      statuses: ['backlog', 'done'],
      activityPreset: 'custom',
      activitySince: '2026-09-01',
      activityUntil: '2026-09-20',
    };
    const params = writeBoardFilters(
      new URLSearchParams({ ticket: 't-1' }),
      filters,
    );
    expect(params.get('ticket')).toBe('t-1');
    expect(parseBoardFilters(params)).toEqual(filters);
  });

  it('omits default values from the URL', () => {
    const params = writeBoardFilters(
      new URLSearchParams({ ticket: 't-1' }),
      DEFAULT_BOARD_FILTERS,
    );
    expect([...params.keys()]).toEqual(['ticket']);
  });

  it('ignores invalid status and visibility values', () => {
    const params = new URLSearchParams({
      status: 'backlog,nope,done',
      visibility: 'ghost',
      activity: 'nope',
    });
    expect(parseBoardFilters(params)).toEqual({
      ...DEFAULT_BOARD_FILTERS,
      statuses: ['backlog', 'done'],
    });
  });
});

describe('countActiveBoardFilters', () => {
  it('counts each non-default dimension once', () => {
    expect(countActiveBoardFilters(DEFAULT_BOARD_FILTERS)).toBe(0);
    expect(
      countActiveBoardFilters({
        q: 'x',
        visibility: 'archived',
        statuses: ['ready'],
        activityPreset: '7d',
        activitySince: null,
        activityUntil: null,
      }),
    ).toBe(4);
  });

  it('does not count empty custom activity', () => {
    expect(
      countActiveBoardFilters({
        ...DEFAULT_BOARD_FILTERS,
        activityPreset: 'custom',
      }),
    ).toBe(0);
  });
});

describe('includeArchivedForVisibility', () => {
  it('fetches archived only when visibility is not active-only', () => {
    expect(includeArchivedForVisibility('active')).toBe(false);
    expect(includeArchivedForVisibility('include_archived')).toBe(true);
    expect(includeArchivedForVisibility('archived')).toBe(true);
  });
});

describe('activityWindow', () => {
  const now = new Date('2026-09-28T12:00:00.000Z');

  it('returns relative windows for presets', () => {
    expect(activityWindow({ ...DEFAULT_BOARD_FILTERS, activityPreset: '7d' }, now)).toEqual({
      since: new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000),
      until: null,
    });
  });

  it('parses custom local day bounds', () => {
    const { since, until } = activityWindow(
      {
        ...DEFAULT_BOARD_FILTERS,
        activityPreset: 'custom',
        activitySince: '2026-09-01',
        activityUntil: '2026-09-02',
      },
      now,
    );
    expect(since?.getFullYear()).toBe(2026);
    expect(since?.getMonth()).toBe(8);
    expect(since?.getDate()).toBe(1);
    expect(since?.getHours()).toBe(0);
    expect(until?.getDate()).toBe(2);
    expect(until?.getHours()).toBe(23);
  });
});

describe('filterTickets', () => {
  const now = new Date('2026-09-28T12:00:00.000Z');
  const tickets = [
    ticket({
      id: 'a',
      title: 'Auth login',
      status: 'backlog',
      lastActivityAt: '2026-09-25T00:00:00.000Z',
    }),
    ticket({
      id: 'b',
      title: 'Billing export',
      status: 'done',
      lastActivityAt: '2026-08-01T00:00:00.000Z',
      archivedAt: '2026-09-20T00:00:00.000Z',
    }),
    ticket({
      id: 'c',
      title: 'Auth logout',
      status: 'in_progress',
      lastActivityAt: '2026-09-27T00:00:00.000Z',
    }),
  ];

  it('filters by search, status, visibility, and activity', () => {
    const result = filterTickets(
      tickets,
      {
        q: 'auth',
        visibility: 'active',
        statuses: ['backlog', 'in_progress'],
        activityPreset: '7d',
        activitySince: null,
        activityUntil: null,
      },
      now,
    );
    expect(result.map((t) => t.id)).toEqual(['a', 'c']);
  });

  it('keeps only archived tickets when visibility is archived', () => {
    const result = filterTickets(
      tickets,
      { ...DEFAULT_BOARD_FILTERS, visibility: 'archived' },
      now,
    );
    expect(result.map((t) => t.id)).toEqual(['b']);
  });
});
