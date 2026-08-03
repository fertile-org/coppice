import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { TicketHierarchy } from './ticketHierarchy';
import { TicketCard } from './TicketCard';
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
) {
  const result = render(
    <DndContext>
      <TicketCard
        ticket={{ ...ticket, ...ticketOverrides }}
        hierarchy={hierarchy}
        onOpen={onOpen}
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
