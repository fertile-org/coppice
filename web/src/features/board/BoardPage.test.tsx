import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { BoardPage } from './BoardPage';

vi.mock('./useReviewCapacity', () => ({
  useReviewCapacity: () => ({
    saved: {
      name: 'Test Board',
      nameLabel: 'Name',
      reviewCap: null,
      countInReview: false,
      waitingOnReview: null,
      boardSettingsLabel: 'Board settings',
      reviewCapLabel: 'Review cap',
      reviewCapHint: 'Leave it empty for no cap.',
      countInReviewLabel: 'Also count In Review',
      reviewCapInvalid: 'Invalid cap.',
    },
    draft: { cap: '', countInReview: false },
    setDraft: () => {},
    error: null,
    commit: () => true,
    isPending: false,
  }),
}));
import type { Ticket } from './useTickets';

// TicketCard calls planCardState once per render: count renders by ticket id.
const cardRenders = vi.hoisted(() => ({ ids: [] as string[] }));
vi.mock('../plan/planCardState', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../plan/planCardState')>();
  return {
    ...actual,
    planCardState: (ticket: Parameters<typeof actual.planCardState>[0]) => {
      cardRenders.ids.push((ticket as { id?: string }).id ?? '');
      return actual.planCardState(ticket);
    },
  };
});

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
  useStartPlanRun: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn() }),
}));

const boardsState = vi.hoisted(() => ({
  data: [{ id: 'board-1', name: 'Launch plan' }] as { id: string; name: string }[] | undefined,
  isPending: false,
}));

vi.mock('../boards/useBoards', () => ({
  setLastBoardId: vi.fn(),
  useBoards: () => boardsState,
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [{ id: 'agent-fe', name: 'Frontend Engineer' }],
  }),
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: [] }),
}));

vi.mock('../tickets/TicketDrawer', () => ({
  TicketDrawer: ({
    open,
    parentTicket,
  }: {
    open: boolean;
    parentTicket?: { id: string; title: string } | null;
  }) => (
    <div data-testid="ticket-drawer-parent" data-open={String(open)}>
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
        <ToastProvider>
          <Routes>
            <Route path="/boards/:boardId" element={<BoardPage />} />
          </Routes>
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('BoardPage heading', () => {
  beforeEach(() => {
    ticketsState.active = [];
    ticketsState.archived = [];
  });
  afterEach(() => {
    boardsState.data = [{ id: 'board-1', name: 'Launch plan' }];
    boardsState.isPending = false;
  });

  it('shows the name of the open board', () => {
    renderBoard();
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Launch plan');
  });

  it('does not flash a generic title while boards load', () => {
    boardsState.data = undefined;
    boardsState.isPending = true;
    renderBoard();
    expect(screen.getByRole('heading', { level: 1 })).not.toHaveTextContent('Board');
  });

  it('falls back to Board when the board is not in the list', () => {
    boardsState.data = [];
    renderBoard();
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Board');
  });
});

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

  it('keeps one of the ticket, filter and settings panels open at a time', () => {
    renderBoard('/boards/board-1?ticket=middle');
    expect(screen.getByTestId('ticket-drawer-parent')).toHaveAttribute(
      'data-open',
      'true',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Board settings' }));
    expect(
      screen.getByRole('dialog', { name: 'Board settings' }),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('ticket-drawer-parent'),
    ).toHaveAttribute('data-open', 'false');

    fireEvent.click(
      screen.getByRole('button', { name: 'Filters', hidden: true }),
    );
    expect(screen.getByRole('dialog', { name: 'Board filters' })).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Board settings' })).toBeNull();

    fireEvent.click(
      screen.getByRole('button', { name: 'Board settings', hidden: true }),
    );
    expect(screen.getByRole('dialog', { name: 'Board settings' })).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Board filters' })).toBeNull();

    fireEvent.click(screen.getByText('Middle ticket'));
    expect(screen.queryByRole('dialog', { name: 'Board settings' })).toBeNull();
    expect(screen.getByTestId('ticket-drawer-parent')).toHaveAttribute(
      'data-open',
      'true',
    );
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

describe('BoardPage render cost when opening a ticket', () => {
  beforeEach(() => {
    ticketsState.lastIncludeArchived = false;
    ticketsState.archived = [];
    ticketsState.active = [
      makeTicket({ id: 'a', title: 'Ticket A', status: 'backlog' }),
      makeTicket({ id: 'b', title: 'Ticket B', status: 'backlog' }),
      makeTicket({ id: 'c', title: 'Ticket C', status: 'backlog' }),
      makeTicket({ id: 'd', title: 'Ticket D', status: 'in_progress' }),
      makeTicket({ id: 'e', title: 'Ticket E', status: 'in_progress' }),
    ];
  });

  it('re-renders only the cards whose focus changed, on the first click', () => {
    renderBoard();
    cardRenders.ids = [];

    // focusState is still null here: the default focus is card A.
    fireEvent.click(screen.getByText('Ticket E'));

    expect(screen.getByTestId('ticket-drawer-parent')).toHaveAttribute(
      'data-open',
      'true',
    );
    const rendered = new Set(cardRenders.ids);
    expect(rendered).not.toContain('b');
    expect(rendered).not.toContain('c');
    expect(rendered).not.toContain('d');
    // A loses the tab stop and E gains it.
    expect([...rendered].sort()).toEqual(['a', 'e']);
  });

  it('does not re-render any card when the drawer opens on the focused card', () => {
    renderBoard();
    fireEvent.focus(screen.getByText('Ticket D').closest('[role="button"]')!);
    cardRenders.ids = [];

    fireEvent.click(screen.getByText('Ticket D'));

    expect(screen.getByTestId('ticket-drawer-parent')).toHaveAttribute(
      'data-open',
      'true',
    );
    expect(cardRenders.ids).toEqual([]);
  });
});

function doneTickets(count: number, titlePrefix = 'Done'): Ticket[] {
  return Array.from({ length: count }, (_, index) =>
    makeTicket({
      id: `${titlePrefix}-${String(index).padStart(2, '0')}`,
      title: `${titlePrefix} ${String(index).padStart(2, '0')}`,
      status: 'done',
      updatedAt: new Date(Date.UTC(2026, 9, 31, 12, 0, 0) - index * 60_000).toISOString(),
    }),
  );
}

describe('BoardPage Done column limit', () => {
  beforeEach(() => {
    ticketsState.lastIncludeArchived = false;
    ticketsState.archived = [];
  });

  it('shows the 10 newest Done tickets and the full count in the header', () => {
    ticketsState.active = [
      ...doneTickets(32).reverse(),
      makeTicket({ id: 'backlog-1', title: 'Backlog card', status: 'backlog' }),
    ];

    renderBoard();

    const done = screen.getByRole('region', { name: 'Done' });
    expect(within(done).getByText('32')).toBeVisible();
    expect(within(done).getByText('Showing 10 of 32')).toBeVisible();
    expect(within(done).getByRole('button', { name: 'Show 10 more' })).toBeVisible();
    expect(within(done).getByText('Done 00')).toBeVisible();
    expect(within(done).getByText('Done 09')).toBeVisible();
    expect(within(done).queryByText('Done 10')).toBeNull();

    const backlog = screen.getByRole('region', { name: 'Backlog' });
    expect(within(backlog).getByText('Backlog card')).toBeVisible();
    expect(within(backlog).queryByText(/Showing \d+ of \d+/)).toBeNull();
    expect(within(backlog).queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
  });

  it('adds up to 10 cards per click and removes the button when all are shown', () => {
    ticketsState.active = doneTickets(22);

    renderBoard();

    const done = screen.getByRole('region', { name: 'Done' });
    fireEvent.click(within(done).getByRole('button', { name: 'Show 10 more' }));
    expect(within(done).getByText('Showing 20 of 22')).toBeVisible();
    expect(within(done).getByRole('button', { name: 'Show 2 more' })).toBeVisible();
    expect(within(done).getByText('Done 19')).toBeVisible();
    expect(within(done).queryByText('Done 20')).toBeNull();

    fireEvent.click(within(done).getByRole('button', { name: 'Show 2 more' }));
    expect(within(done).getByText('Done 21')).toBeVisible();
    expect(within(done).queryByText(/Showing \d+ of \d+/)).toBeNull();
    expect(within(done).queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
  });

  it('shows every ticket and no footer when Done has 10 or fewer', () => {
    ticketsState.active = doneTickets(8);

    renderBoard();

    const done = screen.getByRole('region', { name: 'Done' });
    expect(within(done).getByText('8')).toBeVisible();
    expect(within(done).getAllByText(/^Done \d+$/)).toHaveLength(8);
    expect(within(done).queryByText(/Showing \d+ of \d+/)).toBeNull();
    expect(within(done).queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
  });

  it('applies the page limit and header count to matching tickets only', () => {
    ticketsState.active = [
      ...doneTickets(12, 'Keep'),
      ...doneTickets(12, 'Hide'),
    ];

    renderBoard();

    fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    fireEvent.change(screen.getByLabelText('Search'), {
      target: { value: 'Keep' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));

    const done = screen.getByRole('region', { name: 'Done' });
    expect(within(done).getByText('12')).toBeVisible();
    expect(within(done).getByText('Showing 10 of 12')).toBeVisible();
    expect(within(done).getByRole('button', { name: 'Show 2 more' })).toBeVisible();
    expect(within(done).getByText('Keep 00')).toBeVisible();
    expect(within(done).queryByText('Keep 10')).toBeNull();
    expect(within(done).queryByText('Hide 00')).toBeNull();
  });
});

describe('BoardPage split swimlanes', () => {
  const family = {
    parentTicketId: 'parent',
    parentTitle: 'Ship the gate',
    count: 3,
    mergedCount: 1,
    autoStart: true,
    activeChildId: 'child-ready',
    nextActor: 'you' as const,
    children: [],
  };

  beforeEach(() => {
    ticketsState.lastIncludeArchived = false;
    ticketsState.archived = [];
    ticketsState.active = [
      makeTicket({
        id: 'plain',
        title: 'Plain ticket',
        status: 'backlog',
      }),
      makeTicket({
        id: 'parent',
        title: 'Ship the gate',
        status: 'in_progress',
        splitProgress: family,
      }),
      makeTicket({
        id: 'child-ready',
        title: 'Open the gate',
        status: 'ready',
        parentTicketId: 'parent',
        splitProgress: { ...family, position: 1 },
      }),
      makeTicket({
        id: 'child-backlog',
        title: 'Write the notice',
        status: 'backlog',
        parentTicketId: 'parent',
        splitProgress: { ...family, position: 2 },
      }),
      makeTicket({
        id: 'child-done',
        title: 'Survey the site',
        status: 'done',
        parentTicketId: 'parent',
        splitProgress: { ...family, position: 3 },
      }),
    ];
  });

  it('renders the parent and its children in one row, under their own columns', () => {
    renderBoard();

    const lane = screen.getByRole('region', { name: 'Ship the gate' });
    expect(within(lane).getByText('1/3 merged')).toBeVisible();
    expect(within(lane).getByText('Needs review')).toBeVisible();

    const row = within(lane).getByTestId('swimlane-row');
    expect(within(row).getByTestId('swimlane-cell-in_progress')).toHaveTextContent(
      'Ship the gate',
    );
    expect(within(row).getByTestId('swimlane-cell-ready')).toHaveTextContent(
      'Open the gate',
    );
    expect(within(row).getByTestId('swimlane-cell-backlog')).toHaveTextContent(
      'Write the notice',
    );
    expect(within(row).getByTestId('swimlane-cell-done')).toHaveTextContent(
      'Survey the site',
    );
    expect(within(row).getByText('↳ 2/3 · Ship the gate')).toBeVisible();
    expect(within(row).getByText('↳ 1/3 · Ship the gate')).toBeVisible();
    expect(within(row).getByText('↳ 3/3 · Ship the gate')).toBeVisible();

    const backlog = screen.getByRole('region', { name: 'Backlog' });
    expect(within(backlog).getByText('Plain ticket')).toBeVisible();
    expect(within(backlog).queryByText('Write the notice')).toBeNull();
    expect(within(backlog).getByText('1')).toBeVisible();
    expect(
      within(screen.getByRole('region', { name: 'Done' })).queryByText('Survey the site'),
    ).toBeNull();
  });

  it('collapses and expands the lane without moving an ordinary ticket', () => {
    renderBoard();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse Ship the gate' }));

    expect(
      screen.getByRole('button', { name: 'Expand Ship the gate' }),
    ).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('swimlane-row')).toBeNull();
    expect(screen.queryByText('Open the gate')).toBeNull();
    expect(screen.getByText('Plain ticket')).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Expand Ship the gate' }));

    expect(screen.getByTestId('swimlane-row')).toBeVisible();
    expect(screen.getByText('Open the gate')).toBeVisible();
    expect(screen.getByText('Plain ticket')).toBeVisible();
  });

  it('omits the needs-review badge when an agent acts next', () => {
    ticketsState.active = ticketsState.active.map((ticket) =>
      ticket.splitProgress
        ? { ...ticket, splitProgress: { ...ticket.splitProgress, nextActor: 'agent' } }
        : ticket,
    );

    renderBoard();

    expect(screen.getByText('1/3 merged')).toBeVisible();
    expect(screen.queryByText('Needs review')).toBeNull();
  });
});
