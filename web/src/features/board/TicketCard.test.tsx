import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';
import { PLAN_COPY } from '../plan/copy';
import type { TicketHierarchy } from './ticketHierarchy';
import { resolveAssigneeName, TicketCard } from './TicketCard';
import type { Ticket } from './useTickets';

vi.mock('../tickets/GitStatusRow', () => ({
  GitStatusRow: () => <div data-testid="git-status-row" />,
}));

const ticket: Ticket = {
  id: 'ticket-1',
  boardId: 'board-1',
  title: 'Implement the board hierarchy',
  description: '',
  status: 'in_progress',
  createdBy: 'user',
  createdAt: '2026-08-03T00:00:00.000Z',
  updatedAt: '2026-08-03T00:00:00.000Z',
  lastActivityAt: '2026-08-03T00:00:00.000Z',
};

function renderCard(
  hierarchy?: TicketHierarchy,
  ticketOverrides: Partial<Ticket> = {},
  onOpen = vi.fn(),
  assigneeName?: string,
) {
  const result = render(
    <DndContext>
      <TicketCard
        ticket={{ ...ticket, ...ticketOverrides }}
        hierarchy={hierarchy}
        onOpen={onOpen}
        assigneeName={assigneeName}
      />
    </DndContext>,
  );
  return { ...result, onOpen };
}

describe('TicketCard hierarchy cues', () => {
  it('renders a child row above the ticket title with full accessible text', () => {
    const parentTitle =
      'A deliberately long parent title that will truncate visually on a narrow card';
    renderCard({
      parent: { id: 'parent-1', title: parentTitle },
      parentUnavailable: false,
      directChildCount: 0,
      doneChildCount: 0,
    });

    const childLabel = screen.getByText(`Child of ${parentTitle}`);
    expect(childLabel).toHaveAttribute('title', parentTitle);
  });

  it('renders direct-child progress for a parent ticket', () => {
    renderCard({
      parent: null,
      parentUnavailable: false,
      directChildCount: 3,
      doneChildCount: 1,
    }, { priority: 'high' });

    expect(screen.getByText('Parent · 3 children · 1/3 done')).toBeVisible();
  });

  it('names a split child by position and parent', () => {
    renderCard(
      {
        parent: { id: 'parent-1', title: 'Ship the gate' },
        parentUnavailable: false,
        directChildCount: 1,
        doneChildCount: 0,
      },
      {
        splitProgress: {
          parentTicketId: 'parent-1',
          parentTitle: 'Ship the gate',
          position: 2,
          count: 3,
          mergedCount: 1,
          autoStart: true,
          nextActor: 'agent',
          children: [],
        },
      },
    );

    const label = screen.getByText('↳ 2 of 3');
    expect(label).toBeVisible();
    expect(label).toHaveAttribute('title', 'Ship the gate');
    expect(screen.queryByText('Child of Ship the gate')).toBeNull();
    expect(screen.queryByText(/^Parent ·/)).toBeNull();
  });

  it('renders the fallback for an unresolved parent', () => {
    renderCard({
      parent: null,
      parentUnavailable: true,
      directChildCount: 0,
      doneChildCount: 0,
    });

    expect(
      screen.getByText('Child ticket · Parent unavailable'),
    ).toBeVisible();
  });

  it('opens the card from Enter and Space', () => {
    const onOpen = vi.fn();
    renderCard(undefined, {}, onOpen);
    const card = screen.getByRole('button');
    fireEvent.keyDown(card, { key: 'Enter' });
    fireEvent.keyDown(card, { key: ' ' });
    expect(onOpen).toHaveBeenNthCalledWith(1, ticket.id);
    expect(onOpen).toHaveBeenNthCalledWith(2, ticket.id);
  });
});

describe('TicketCard assignee and priority', () => {
  it('shows the assignee name when provided and omits it when unset', () => {
    const { rerender } = renderCard(undefined, {}, vi.fn(), 'Frontend Engineer');

    expect(screen.getByText('Frontend Engineer')).toBeVisible();

    rerender(
      <DndContext>
        <TicketCard ticket={ticket} onOpen={vi.fn()} />
      </DndContext>,
    );
    expect(screen.queryByText('Frontend Engineer')).toBeNull();
  });

  it('resolveAssigneeName omits unset ids and falls back for missing agents', () => {
    const agents = new Map([['agent-1', 'FE']]);
    expect(resolveAssigneeName(undefined, agents)).toBeUndefined();
    expect(resolveAssigneeName('agent-1', agents)).toBe('FE');
    expect(resolveAssigneeName('missing', agents)).toBe('Unknown agent');
  });

  it('styles substatus pills', () => {
    renderCard(undefined, {
      substatus: 'blocked_by_error',
      substatusDisplay: { label: 'Blocked — error' },
    });
    expect(screen.getByText('Blocked — error').parentElement).toHaveClass('text-danger');
  });
});

describe('TicketCard plan labels', () => {
  it('shows the locked labels and starts a plan without opening the card', () => {
    const onOpen = vi.fn();
    const onRunPlan = vi.fn();
    const { rerender } = render(
      <DndContext>
        <TicketCard
          ticket={{ ...ticket, status: 'backlog' }}
          onOpen={onOpen}
          onRunPlan={onRunPlan}
        />
      </DndContext>,
    );

    expect(screen.getByText(PLAN_COPY.planNeedsRun)).toBeVisible();
    fireEvent.click(
      screen.getByRole('button', { name: PLAN_COPY.runAgent }),
    );
    expect(onRunPlan).toHaveBeenCalledWith(ticket.id);
    expect(onOpen).not.toHaveBeenCalled();

    rerender(
      <DndContext>
        <TicketCard
          ticket={{
            ...ticket,
            status: 'plan_review',
            latestPlanRunStatus: 'running',
          }}
          onOpen={onOpen}
        />
      </DndContext>,
    );
    expect(screen.getByText(PLAN_COPY.writingPlan)).toBeVisible();

    rerender(
      <DndContext>
        <TicketCard
          ticket={{ ...ticket, status: 'plan_review' }}
          onOpen={onOpen}
        />
      </DndContext>,
    );
    expect(screen.getByText(PLAN_COPY.waitingForApproval)).toBeVisible();
  });

  it('assigns the suggested PM without opening the card', () => {
    const onOpen = vi.fn();
    const onAssignSuggested = vi.fn();
    render(
      <DndContext>
        <TicketCard
          ticket={{
            ...ticket,
            status: 'backlog',
            suggestedAssignee: {
              id: '00000000-0000-0000-0000-000000000010',
              name: 'PM',
            },
          }}
          onOpen={onOpen}
          onRunPlan={vi.fn()}
          onAssignSuggested={onAssignSuggested}
        />
      </DndContext>,
    );

    const button = screen.getByRole('button', { name: PLAN_COPY.assignPm });
    expect(screen.queryByRole('button', { name: PLAN_COPY.runAgent })).toBeNull();
    fireEvent.pointerDown(button);
    fireEvent.click(button);
    expect(onAssignSuggested).toHaveBeenCalledWith(
      ticket.id,
      '00000000-0000-0000-0000-000000000010',
    );
    expect(onOpen).not.toHaveBeenCalled();
  });
});

describe('TicketCard Done git flags', () => {
  function renderDone(overrides: Partial<Ticket>) {
    return render(
      <QueryClientProvider client={new QueryClient()}>
        <DndContext>
          <TicketCard ticket={{ ...ticket, ...overrides }} onOpen={vi.fn()} />
        </DndContext>
      </QueryClientProvider>,
    );
  }
  const doneBase = { status: 'done' as const, repoId: 'repo-1' };
  const review = (gitState: string, acceptedByCoppice = false) =>
    ({ gitState, acceptedByCoppice }) as unknown as Ticket['humanReview'];

  it('flags needs-action Done cards and leaves merged cards unflagged', () => {
    const { container, unmount } = renderDone({
      ...doneBase,
      humanReview: review('not_merged'),
    });
    expect(container.querySelector('[data-needs-action]')).not.toBeNull();
    unmount();
    const conflict = renderDone({
      ...doneBase,
      humanReview: review('conflict'),
    });
    expect(conflict.container.querySelector('[data-needs-action]')).not.toBeNull();
    conflict.unmount();
    const merged = renderDone({
      ...doneBase,
      humanReview: review('merged'),
    });
    expect(merged.container.querySelector('[data-needs-action]')).toBeNull();
  });


});
