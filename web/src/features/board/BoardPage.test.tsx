import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BoardPage } from './BoardPage';
import type { Ticket } from './useTickets';

const ticketsState = vi.hoisted(() => ({
  active: [] as Ticket[],
  archived: [] as Ticket[],
  lastIncludeArchived: false,
}));

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
  ticketsQueryKey: (boardId: string) => ['tickets', boardId],
  useTickets: (
    _boardId: string | undefined,
    includeArchived = false,
  ) => {
    ticketsState.lastIncludeArchived = includeArchived;
    return {
      data: includeArchived
        ? [...ticketsState.active, ...ticketsState.archived]
        : ticketsState.active,
      isLoading: false,
      isError: false,
      refetch: vi.fn(),
    };
  },
  useCreateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn() }),
}));

vi.mock('../boards/useBoards', () => ({
  setLastBoardId: vi.fn(),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [{ id: 'agent-fe', name: 'Frontend Engineer' }],
  }),
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
    boardId: 'board-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-08-03T00:00:00.000Z',
    updatedAt: '2026-08-03T00:00:00.000Z',
    lastActivityAt: '2026-08-03T00:00:00.000Z',
    ...overrides,
  };
}

function renderBoard(initialEntry = '/boards/board-1') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <QueryClientProvider client={client}>
        <Routes>
          <Route path="/boards/:boardId" element={<BoardPage />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('BoardPage ticket hierarchy', () => {
  beforeEach(() => {
    ticketsState.lastIncludeArchived = false;
    ticketsState.archived = [];
    ticketsState.active = [
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
    renderBoard('/boards/board-1?ticket=middle');

    expect(screen.getByTestId('ticket-drawer-parent')).toHaveTextContent(
      'Root ticket',
    );
  });

  it('shows assignee names on cards and omits them when unassigned', () => {
    ticketsState.active = [
      makeTicket({
        id: 'assigned',
        title: 'Assigned ticket',
        status: 'backlog',
        assigneeAgentId: 'agent-fe',
        priority: 'high',
      }),
      makeTicket({
        id: 'unassigned',
        title: 'Unassigned ticket',
        status: 'backlog',
        assigneeAgentId: 'missing-agent',
      }),
      makeTicket({
        id: 'plain',
        title: 'Plain ticket',
        status: 'ready',
      }),
    ];

    renderBoard();

    const backlog = screen.getByRole('region', { name: 'Backlog' });
    expect(within(backlog).getByText('Frontend Engineer')).toBeVisible();
    expect(within(backlog).getByText('Unknown agent')).toBeVisible();
    expect(within(backlog).getByText('high')).toHaveAttribute(
      'data-priority',
      'high',
    );

    const ready = screen.getByRole('region', { name: 'Ready' });
    expect(within(ready).queryByText('Frontend Engineer')).toBeNull();
    expect(within(ready).queryByText('Unknown agent')).toBeNull();
  });

  it('hides archived tickets by default and shows them via the filter drawer', () => {
    ticketsState.active = [
      makeTicket({
        id: 'active-1',
        title: 'Active ticket',
        status: 'backlog',
      }),
    ];
    ticketsState.archived = [
      makeTicket({
        id: 'archived-1',
        title: 'Archived ticket',
        status: 'done',
        archivedAt: '2026-09-20T00:00:00.000Z',
      }),
    ];

    renderBoard();

    expect(screen.getByText('Active ticket')).toBeVisible();
    expect(screen.queryByText('Archived ticket')).toBeNull();
    expect(ticketsState.lastIncludeArchived).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    fireEvent.click(screen.getByRole('radio', { name: 'Include archived' }));

    expect(ticketsState.lastIncludeArchived).toBe(true);
    expect(screen.getByText('Archived ticket')).toBeVisible();
    expect(
      within(screen.getByRole('region', { name: 'Done', hidden: true })).getByText('Archived'),
    ).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Filters, 1 active', hidden: true }),
    ).toBeVisible();
  });

  it('filters cards by search and status without hiding columns', () => {
    ticketsState.active = [
      makeTicket({
        id: 'auth',
        title: 'Auth login',
        status: 'backlog',
      }),
      makeTicket({
        id: 'billing',
        title: 'Billing export',
        status: 'ready',
      }),
    ];

    renderBoard();

    fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    fireEvent.change(screen.getByLabelText('Search'), {
      target: { value: 'auth' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Backlog' }));

    expect(screen.getByText('Auth login')).toBeVisible();
    expect(screen.queryByText('Billing export')).toBeNull();
    expect(screen.getByRole('region', { name: 'Ready', hidden: true })).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Filters, 2 active', hidden: true }),
    ).toBeVisible();
  });
});
