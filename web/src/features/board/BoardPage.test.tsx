import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BoardPage } from './BoardPage';
import type { Ticket } from './useTickets';

const ticketsState = vi.hoisted(() => ({ tickets: [] as Ticket[] }));

vi.mock('@dnd-kit/core', () => ({
  DndContext: ({
    children,
    onDragStart,
  }: {
    children: React.ReactNode;
    onDragStart: (event: {
      active: { id: string; data: { current: undefined } };
    }) => void;
  }) => (
    <div>
      <button
        type="button"
        onClick={() =>
          onDragStart({
            active: { id: 'middle', data: { current: undefined } },
          })
        }
      >
        Start test drag
      </button>
      {children}
    </div>
  ),
  DragOverlay: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="drag-overlay">{children}</div>
  ),
  PointerSensor: function PointerSensor() {},
  useSensor: () => ({}),
  useSensors: (...sensors: unknown[]) => sensors,
  useDroppable: () => ({ setNodeRef: vi.fn(), isOver: false }),
  useDraggable: () => ({
    attributes: {},
    listeners: {},
    setNodeRef: vi.fn(),
    transform: null,
    isDragging: false,
  }),
}));

vi.mock('./useTickets', () => ({
  ticketsQueryKey: (projectId: string) => ['tickets', projectId],
  useTickets: () => ({
    data: ticketsState.tickets,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useCreateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn() }),
}));

vi.mock('../projects/useProjects', () => ({
  setLastProjectId: vi.fn(),
}));

vi.mock('../tickets/TicketDrawer', () => ({
  TicketDrawer: ({
    parentTicket,
  }: {
    parentTicket?: { id: string; title: string } | null;
  }) => (
    <div data-testid="ticket-drawer-parent">
      {parentTicket?.title ?? 'No parent'}
    </div>
  ),
}));

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

function renderBoard(initialEntry = '/projects/project-1/board') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider client={client}>
        <Routes>
          <Route path="/projects/:projectId/board" element={<BoardPage />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('BoardPage ticket hierarchy', () => {
  beforeEach(() => {
    ticketsState.tickets = [
      makeTicket({
        id: 'root',
        title: 'Root ticket',
        status: 'backlog',
      }),
      makeTicket({
        id: 'middle',
        title: 'Middle ticket',
        status: 'in_progress',
        parentTicketId: 'root',
      }),
      makeTicket({
        id: 'leaf',
        title: 'Leaf ticket',
        status: 'done',
        parentTicketId: 'middle',
      }),
    ];
  });

  it('keeps cross-column cards independent and repeats both cues in the drag overlay', () => {
    renderBoard();

    expect(
      within(screen.getByRole('region', { name: 'Backlog' })).getByText(
        'Root ticket',
      ),
    ).toBeVisible();
    const inProgressColumn = screen.getByRole('region', {
      name: 'In Progress',
    });
    expect(within(inProgressColumn).getByText('Middle ticket')).toBeVisible();
    expect(within(inProgressColumn).getByText('Child of Root ticket')).toBeVisible();
    expect(
      within(inProgressColumn).getByText('Parent · 1 children · 1/1 done'),
    ).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Start test drag' }));

    const overlay = screen.getByTestId('drag-overlay');
    expect(within(overlay).getByText('Child of Root ticket')).toBeVisible();
    expect(
      within(overlay).getByText('Parent · 1 children · 1/1 done'),
    ).toBeVisible();
  });

  it('passes the selected child parent from the existing board data to the drawer', () => {
    renderBoard('/projects/project-1/board?ticket=middle');

    expect(screen.getByTestId('ticket-drawer-parent')).toHaveTextContent(
      'Root ticket',
    );
  });
});
