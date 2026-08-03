import { describe, expect, it } from 'vitest';
import type { Ticket } from './useTickets';
import { buildTicketHierarchyIndex } from './ticketHierarchy';

function makeTicket(
  overrides: Pick<Ticket, 'id' | 'title' | 'status'> & Partial<Ticket>,
): Ticket {
  return {
    projectId: 'project-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-08-03T00:00:00.000Z',
    updatedAt: '2026-08-03T00:00:00.000Z',
    lastActivityAt: '2026-08-03T00:00:00.000Z',
    ...overrides,
  };
}

describe('buildTicketHierarchyIndex', () => {
  it('returns empty relationship metadata for an unrelated ticket', () => {
    const standalone = makeTicket({
      id: 'standalone',
      title: 'Standalone ticket',
      status: 'backlog',
    });

    const index = buildTicketHierarchyIndex([standalone]);

    expect(index.get(standalone.id)).toEqual({
      parent: null,
      parentUnavailable: false,
      directChildCount: 0,
      doneChildCount: 0,
    });
  });

  it('resolves cross-column relationships and counts only direct done children', () => {
    const parent = makeTicket({
      id: 'parent',
      title: 'Parent ticket',
      status: 'backlog',
    });
    const doneChild = makeTicket({
      id: 'done-child',
      title: 'Done child',
      status: 'done',
      parentTicketId: parent.id,
    });
    const reviewChild = makeTicket({
      id: 'review-child',
      title: 'Review child',
      status: 'in_review',
      parentTicketId: parent.id,
    });
    const grandchild = makeTicket({
      id: 'grandchild',
      title: 'Grandchild',
      status: 'done',
      parentTicketId: reviewChild.id,
    });

    const index = buildTicketHierarchyIndex([
      parent,
      doneChild,
      reviewChild,
      grandchild,
    ]);

    expect(index.get(parent.id)).toEqual({
      parent: null,
      parentUnavailable: false,
      directChildCount: 2,
      doneChildCount: 1,
    });
    expect(index.get(reviewChild.id)).toEqual({
      parent: { id: parent.id, title: parent.title },
      parentUnavailable: false,
      directChildCount: 1,
      doneChildCount: 1,
    });
    expect(index.get(grandchild.id)?.parent).toEqual({
      id: reviewChild.id,
      title: reviewChild.title,
    });
  });

  it('marks a child whose parent is absent from the board data', () => {
    const child = makeTicket({
      id: 'orphaned-child',
      title: 'Orphaned child',
      status: 'ready',
      parentTicketId: 'missing-parent',
    });

    const index = buildTicketHierarchyIndex([child]);

    expect(index.get(child.id)).toEqual({
      parent: null,
      parentUnavailable: true,
      directChildCount: 0,
      doneChildCount: 0,
    });
  });
});
