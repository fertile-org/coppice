import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { TicketHierarchy } from './ticketHierarchy';
import { resolveAssigneeName, TicketCard } from './TicketCard';
import type { Ticket } from './useTickets';

const ticket: Ticket = {
  id: 'ticket-1',
  projectId: 'project-1',
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
  it('renders no hierarchy rows for an unrelated ticket', () => {
    renderCard({
      parent: null,
      parentUnavailable: false,
      directChildCount: 0,
      doneChildCount: 0,
    });

    expect(screen.queryByText(/^Child of /)).toBeNull();
    expect(screen.queryByText(/^Parent ·/)).toBeNull();
  });

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
    const ticketTitle = screen.getByText(ticket.title);

    expect(childLabel).toHaveClass('truncate');
    expect(childLabel).toHaveAttribute('title', parentTitle);
    expect(
      childLabel.compareDocumentPosition(ticketTitle) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(childLabel.closest('div')?.querySelector('svg')).toHaveAttribute(
      'aria-hidden',
      'true',
    );
  });

  it('renders direct-child progress for a parent ticket', () => {
    renderCard({
      parent: null,
      parentUnavailable: false,
      directChildCount: 3,
      doneChildCount: 1,
    }, { priority: 'high' });

    const priority = screen.getByText('high');
    const parentLabel = screen.getByText('Parent · 3 children · 1/3 done');
    expect(parentLabel).toBeVisible();
    expect(
      priority.compareDocumentPosition(parentLabel) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('renders both rows when the ticket is both a child and a parent', () => {
    renderCard({
      parent: { id: 'parent-1', title: 'Roadmap parent' },
      parentUnavailable: false,
      directChildCount: 2,
      doneChildCount: 2,
    });

    expect(screen.getByText('Child of Roadmap parent')).toBeVisible();
    const parentLabel = screen.getByText('Parent · 2 children · 2/2 done');
    expect(parentLabel).toBeVisible();
    expect(parentLabel.closest('div')?.querySelector('svg')).toHaveAttribute(
      'aria-hidden',
      'true',
    );
    expect(screen.getAllByRole('button')).toHaveLength(1);
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

  it('retains keyboard card activation and a visible focus ring', () => {
    const onOpen = vi.fn();
    renderCard(undefined, {}, onOpen);
    const card = screen.getByRole('button');

    expect(card).toHaveClass('focus-visible:ring-accent');
    fireEvent.keyDown(card, { key: 'Enter' });
    fireEvent.keyDown(card, { key: ' ' });

    expect(onOpen).toHaveBeenNthCalledWith(1, ticket.id);
    expect(onOpen).toHaveBeenNthCalledWith(2, ticket.id);
  });
});

describe('TicketCard assignee and priority', () => {
  it('shows the assignee name when provided and omits it when unset', () => {
    const { rerender } = renderCard(undefined, {}, vi.fn(), 'Frontend Engineer');

    const assignee = screen.getByText('Frontend Engineer');
    expect(assignee).toBeVisible();
    expect(assignee).toHaveClass('truncate');
    expect(assignee).toHaveAttribute('title', 'Frontend Engineer');

    rerender(
      <DndContext>
        <TicketCard ticket={ticket} onOpen={vi.fn()} />
      </DndContext>,
    );
    expect(screen.queryByText('Frontend Engineer')).toBeNull();
  });

  it('renders the unknown-agent fallback label', () => {
    renderCard(undefined, {}, vi.fn(), 'Unknown agent');
    expect(screen.getByText('Unknown agent')).toBeVisible();
  });

  it('styles low and critical priority badges with distinct token colors', () => {
    const { rerender } = renderCard(undefined, { priority: 'low' });
    const low = screen.getByText('low');
    expect(low).toHaveAttribute('data-priority', 'low');
    expect(low.style.backgroundColor).toBe('var(--badge-priority-low-bg)');
    expect(low.style.color).toBe('var(--badge-priority-low-text)');
    expect(low.style.borderColor).toBe('var(--badge-priority-low-border)');
    expect(low.className).toMatch(/capitalize/);

    rerender(
      <DndContext>
        <TicketCard
          ticket={{ ...ticket, priority: 'critical' }}
          onOpen={vi.fn()}
        />
      </DndContext>,
    );
    const critical = screen.getByText('critical');
    expect(critical).toHaveAttribute('data-priority', 'critical');
    expect(critical.style.backgroundColor).toBe(
      'var(--badge-priority-critical-bg)',
    );
    expect(critical.style.color).toBe('var(--badge-priority-critical-text)');
    expect(critical.style.borderColor).toBe(
      'var(--badge-priority-critical-border)',
    );
  });

  it('resolveAssigneeName omits unset ids and falls back for missing agents', () => {
    const agents = new Map([['agent-1', 'FE']]);
    expect(resolveAssigneeName(undefined, agents)).toBeUndefined();
    expect(resolveAssigneeName('agent-1', agents)).toBe('FE');
    expect(resolveAssigneeName('missing', agents)).toBe('Unknown agent');
  });
});
