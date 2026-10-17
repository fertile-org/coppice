import { describe, expect, it } from 'vitest';
import type { Ticket } from './useTickets';
import {
  acceptShortcutEnabled,
  adjacentColumn,
  approveShortcutEnabled,
  codeReviewEnabled,
  columnIndexForFocus,
  moveTargetStatus,
} from './ticketShortcutRules';

function ticket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: 'tic-1',
    boardId: 'board-1',
    title: 'Ship the gate',
    description: '',
    status: 'backlog',
    createdBy: 'user',
    createdAt: '2026-10-15T00:00:00.000Z',
    updatedAt: '2026-10-15T00:00:00.000Z',
    lastActivityAt: '2026-10-15T00:00:00.000Z',
    ...overrides,
  };
}

describe('ticket shortcut rules', () => {
  it('steps columns from Backlog through Blocked and stops at the ends', () => {
    expect(adjacentColumn('backlog', 1)?.label).toBe('Plan Review');
    expect(adjacentColumn('backlog', -1)).toBeNull();
    expect(adjacentColumn('done', 1)?.label).toBe('Blocked');
    expect(adjacentColumn('blocked', 1)).toBeNull();
    expect(adjacentColumn('in_qa', 1)?.label).toBe('Wait for Human Review');
    expect(adjacentColumn('wait_for_final_review', -1)?.label).toBe('In QA');
    expect(moveTargetStatus(ticket({ status: 'backlog' }), 1)).toBe('plan_review');
    expect(moveTargetStatus(ticket({ status: 'blocked' }), 1)).toBeNull();
    expect(
      moveTargetStatus(
        ticket({ status: 'backlog', archivedAt: '2026-10-15T00:00:00.000Z' }),
        1,
      ),
    ).toBeNull();
    expect(approveShortcutEnabled(ticket({ status: 'plan_review' }))).toBe(true);
    expect(approveShortcutEnabled(ticket({ status: 'plan_review', hasPlan: false }))).toBe(false);
    expect(
      approveShortcutEnabled(
        ticket({
          status: 'plan_review',
          planQuestions: { questions: [{ question: 'Q?' }], answers: [], waiting: true },
        }),
      ),
    ).toBe(false);
    expect(approveShortcutEnabled(ticket({ status: 'ready' }))).toBe(false);
    expect(acceptShortcutEnabled(ticket({ status: 'wait_for_final_review' }))).toBe(true);
    expect(
      acceptShortcutEnabled(
        ticket({
          status: 'wait_for_final_review',
          archivedAt: '2026-10-15T00:00:00.000Z',
        }),
      ),
    ).toBe(false);
    expect(acceptShortcutEnabled(ticket({ status: 'done' }))).toBe(false);
  });

  it('finds the focused column for a card and a heading', () => {
    const columns = [{ ticketIds: ['a'] }, { ticketIds: [] as string[] }];
    expect(columnIndexForFocus(columns, { type: 'card', ticketId: 'a' })).toBe(0);
    expect(columnIndexForFocus(columns, { type: 'heading', column: 1 })).toBe(1);
    expect(columnIndexForFocus(columns, { type: 'card', ticketId: 'missing' })).toBeNull();
  });

  it('enables Open code review when the ticket has a ready repository', () => {
    expect(codeReviewEnabled(ticket(), [])).toBe(false);
    expect(
      codeReviewEnabled(ticket({ repoId: 'repo-1' }), [
        { id: 'repo-1', verificationStatus: 'ready' },
      ]),
    ).toBe(true);
    expect(
      codeReviewEnabled(ticket({ repoId: 'repo-1' }), [
        { id: 'repo-1', verificationStatus: 'path_missing' },
      ]),
    ).toBe(false);
    expect(codeReviewEnabled(ticket({ repoId: 'repo-1' }), [])).toBe(true);
  });
});
