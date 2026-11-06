import { describe, expect, it } from 'vitest';
import type { TicketStatus } from './columns';
import {
  columnTicketIds,
  splitChildLabel,
  splitLanes,
  splitNeedsReview,
  statusFromLaneDropId,
} from './swimlanes';
import type { SplitProgress, Ticket } from './useTickets';

function makeTicket(
  overrides: Pick<Ticket, 'id' | 'title' | 'status'> & Partial<Ticket>,
): Ticket {
  return {
    boardId: 'board-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-08-03T00:00:00.000Z',
    updatedAt: '2026-08-03T00:00:00.000Z',
    lastActivityAt: '2026-08-03T00:00:00.000Z',
    ...overrides,
  };
}

function progress(
  overrides: Partial<SplitProgress> = {},
): SplitProgress {
  return {
    parentTicketId: 'parent',
    parentTitle: 'Ship the gate',
    count: 3,
    mergedCount: 1,
    autoStart: true,
    nextActor: 'you',
    children: [],
    ...overrides,
  };
}

describe('splitLanes', () => {
  const parent = makeTicket({
    id: 'parent',
    title: 'Ship the gate',
    status: 'in_progress',
    splitProgress: progress(),
  });
  const ready = makeTicket({
    id: 'child-ready',
    title: 'Child ready',
    status: 'ready',
    parentTicketId: 'parent',
    splitProgress: progress({ position: 1 }),
  });
  const backlog = makeTicket({
    id: 'child-backlog',
    title: 'Child backlog',
    status: 'backlog',
    parentTicketId: 'parent',
    splitProgress: progress({ position: 2 }),
  });
  const done = makeTicket({
    id: 'child-done',
    title: 'Child done',
    status: 'done',
    parentTicketId: 'parent',
    splitProgress: progress({ position: 3 }),
  });
  const plain = makeTicket({
    id: 'plain',
    title: 'Plain ticket',
    status: 'backlog',
    parentTicketId: 'someone-else',
  });

  const grouped = splitLanes([plain, done, parent, backlog, ready]);

  it('puts a split family in one lane and leaves other tickets in the columns', () => {
    expect(grouped.lanes).toHaveLength(1);
    expect(grouped.lanes[0]?.parentId).toBe('parent');
    expect(grouped.lanes[0]?.tickets.map((ticket) => ticket.id)).toEqual([
      'child-done',
      'parent',
      'child-backlog',
      'child-ready',
    ]);
    expect(grouped.rest.map((ticket) => ticket.id)).toEqual(['plain']);
    expect(grouped.lanes[0]?.progress.position ?? null).toBeNull();
  });

  it('lists lane cards above the column for keyboard order, and skips a collapsed lane', () => {
    const ids = (status: TicketStatus, collapsed = new Set<string>()) =>
      columnTicketIds(
        grouped.lanes,
        collapsed,
        status,
        grouped.rest.filter((ticket) => ticket.status === status),
      );

    expect(ids('backlog')).toEqual(['child-backlog', 'plain']);
    expect(ids('ready')).toEqual(['child-ready']);
    expect(ids('in_progress')).toEqual(['parent']);
    expect(ids('done')).toEqual(['child-done']);
    expect(ids('backlog', new Set(['parent']))).toEqual(['plain']);
  });

  it('returns a finished split to the columns', () => {
    const finishedParent = makeTicket({
      id: 'parent',
      title: 'Ship the gate',
      status: 'done',
      splitProgress: progress({ mergedCount: 3 }),
    });
    const merged = [1, 2, 3].map((position) =>
      makeTicket({
        id: `merged-${position}`,
        title: `Merged ${position}`,
        status: 'done',
        parentTicketId: 'parent',
        splitProgress: progress({ position, mergedCount: 3 }),
      }),
    );

    const finished = splitLanes([finishedParent, ...merged]);

    expect(finished.lanes).toEqual([]);
    expect(finished.rest.map((ticket) => ticket.id)).toEqual([
      'parent',
      'merged-1',
      'merged-2',
      'merged-3',
    ]);

    const parentHidden = splitLanes(merged, [finishedParent, ...merged]);
    expect(parentHidden.lanes).toEqual([]);
    expect(parentHidden.rest.map((ticket) => ticket.id)).toEqual([
      'merged-1',
      'merged-2',
      'merged-3',
    ]);
  });

  it('keeps the lane when a filter hides the parent but the parent is still active', () => {
    const visible = splitLanes([ready, backlog], [parent, ready, backlog]);

    expect(visible.lanes).toHaveLength(1);
    expect(visible.lanes[0]?.tickets.map((ticket) => ticket.id)).toEqual([
      'child-ready',
      'child-backlog',
    ]);
    expect(visible.rest).toEqual([]);
  });
});

describe('split copy and lane drop ids', () => {
  it('labels a child and not the parent', () => {
    expect(splitChildLabel(progress({ position: 2 }))).toBe(
      '↳ 2/3 · Ship the gate',
    );
    expect(splitChildLabel(progress())).toBeNull();
  });

  it('needs review only when the next actor is a person', () => {
    expect(splitNeedsReview(progress({ nextActor: 'you' }))).toBe(true);
    expect(splitNeedsReview(progress({ nextActor: 'agent' }))).toBe(false);
  });

  it('reads a status from a lane drop id', () => {
    expect(statusFromLaneDropId('lane:parent:wait_for_final_review')).toBe(
      'wait_for_final_review',
    );
    expect(statusFromLaneDropId('lane:parent:nope')).toBeNull();
    expect(statusFromLaneDropId('backlog')).toBeNull();
  });
});
