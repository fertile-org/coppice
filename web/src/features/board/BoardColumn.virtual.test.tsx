import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { BoardColumn, VIRTUALIZE_AFTER } from './BoardColumn';
import { BOARD_COLUMNS } from './columns';
import type { Ticket } from './useTickets';

function tickets(count: number): Ticket[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `00000000-0000-0000-0000-${String(i).padStart(12, '0')}`,
    boardId: 'b',
    title: `Card ${i}`,
    status: 'backlog',
    createdBy: 'user',
    createdAt: '2026-10-18T00:00:00Z',
    updatedAt: '2026-10-18T00:00:00Z',
    lastActivityAt: '2026-10-18T00:00:00Z',
  }));
}

function renderColumn(list: Ticket[]) {
  return render(
    <MemoryRouter>
      <ToastProvider>
        <DndContext>
          <BoardColumn
            column={BOARD_COLUMNS[0]}
            tickets={list}
            hierarchyIndex={new Map()}
            agentsById={new Map()}
            onOpenTicket={() => {}}
          />
        </DndContext>
      </ToastProvider>
    </MemoryRouter>,
  );
}

describe('BoardColumn virtualization', () => {
  // jsdom has no layout; give the scroll area and cards a size.
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const height = this.hasAttribute('data-virtual-column') ? 800 : 100;
      return { x: 0, y: 0, top: 0, left: 0, bottom: height, right: 280, width: 280, height, toJSON() {} } as DOMRect;
    });
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-virtual-column') ? 800 : 100;
    });
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(280);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders every card in a short column', () => {
    renderColumn(tickets(VIRTUALIZE_AFTER));
    expect(screen.getAllByText(/^Card \d+$/)).toHaveLength(VIRTUALIZE_AFTER);
  });

  it('mounts only the cards in view for a 2,000-card column', () => {
    renderColumn(tickets(2000));
    const mounted = screen.getAllByText(/^Card \d+$/).length;
    expect(mounted).toBeGreaterThan(0);
    expect(mounted).toBeLessThan(40);
    expect(screen.getByText('2000')).toBeInTheDocument();
  });
});
