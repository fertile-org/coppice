import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation, useSearchParams } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { SkipToBoard } from '../keyboard/SkipToBoard';
import { BoardPage } from './BoardPage';
import type { Ticket } from './useTickets';

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

const ticketsState = vi.hoisted(() => ({
  active: [] as Ticket[],
}));

const updateStatus = vi.hoisted(() => vi.fn());
const createTicket = vi.hoisted(() => vi.fn());
const approveTicketPlan = vi.hoisted(() => vi.fn());
const mergeBranch = vi.hoisted(() => vi.fn());
const gitInfoState = vi.hoisted(() => ({
  current: null as null | {
    defaultBranch: string;
    branches: string[];
    ticketBranch: string;
    worktreePath: string;
    worktreeExists: boolean;
  },
}));
const reposState = vi.hoisted(() => ({
  data: [] as { id: string; verificationStatus: string }[],
}));

vi.mock('@dnd-kit/core', () => ({
  DndContext: ({
    children,
    onDragEnd,
  }: {
    children: React.ReactNode;
    onDragEnd?: (event: {
      active: { id: string };
      over: { id: string } | null;
    }) => void;
  }) => (
    <div>
      <button
        type="button"
        onClick={() =>
          onDragEnd?.({ active: { id: 'alpha' }, over: { id: 'ready' } })
        }
      >
        Drop alpha on Ready
      </button>
      {children}
    </div>
  ),
  DragOverlay: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  PointerSensor: function PointerSensor() {},
  useSensor: () => ({}),
  useSensors: () => [],
  useDroppable: () => ({ setNodeRef: () => undefined, isOver: false }),
  useDraggable: () => ({
    attributes: {},
    listeners: {},
    setNodeRef: () => undefined,
    transform: null,
    isDragging: false,
  }),
}));

vi.mock('./useTickets', () => ({
  ticketsQueryKey: (boardId: string) => ['tickets', boardId],
  useTickets: () => ({
    data: ticketsState.active,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  useCreateTicket: () => ({ mutateAsync: createTicket, isPending: false }),
  useStartPlanRun: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
  useUpdateTicketStatus: () => ({ mutateAsync: updateStatus }),
}));

vi.mock('../boards/useBoards', () => ({
  setLastBoardId: vi.fn(),
  useBoards: () => ({
    data: [{ id: 'board-1', name: 'Launch plan', slug: 'launch', createdAt: '' }],
    isPending: false,
  }),
  useCreateBoard: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: [] }),
}));

vi.mock('../auth/useSession', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../auth/useSession')>()),
  useSession: () => ({
    user: { id: 'u1', email: 'u@example.com', role: 'admin' },
    loading: false,
    desktopMode: false,
    establishSession: vi.fn(),
    logout: vi.fn(),
  }),
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: reposState.data }),
  usePushDefaultBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('../plan/approvePlan', () => ({
  approveTicketPlan: (...args: unknown[]) => approveTicketPlan(...args),
}));

vi.mock('../tickets/useTicket', async () => {
  const actual = await vi.importActual<typeof import('../tickets/useTicket')>(
    '../tickets/useTicket',
  );
  return {
    ...actual,
    useTicketGitInfo: () => ({
      data: gitInfoState.current ?? undefined,
      isLoading: false,
    }),
    useMergeTicketBranch: () => ({ mutateAsync: mergeBranch, isPending: false }),
  };
});

vi.mock('../tickets/TicketDrawer', () => ({
  TicketDrawer: ({
    open = true,
    onClose,
    onRestoreFocus,
  }: {
    open?: boolean;
    onClose: () => void;
    onRestoreFocus?: () => void;
  }) =>
    open ? (
      <div role="dialog" data-shortcut-layer="ticket" data-testid="ticket-drawer">
        <button
          type="button"
          onClick={() => {
            onClose();
            onRestoreFocus?.();
          }}
        >
          Close drawer
        </button>
      </div>
    ) : null,
}));

function makeTicket(
  overrides: Pick<Ticket, 'id' | 'title' | 'status'> & Partial<Ticket>,
): Ticket {
  return {
    boardId: 'board-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-10-15T00:00:00.000Z',
    updatedAt: '2026-10-15T00:00:00.000Z',
    lastActivityAt: '2026-10-15T00:00:00.000Z',
    ...overrides,
  };
}

function LocationProbe() {
  const location = useLocation();
  return (
    <div data-testid="location">
      {location.pathname}
      {location.search}
    </div>
  );
}

function OpenTicketViaUrl({ ticketId }: { ticketId: string }) {
  const [params, setParams] = useSearchParams();
  return (
    <button
      type="button"
      onClick={() => {
        const next = new URLSearchParams(params);
        next.set('ticket', ticketId);
        setParams(next);
      }}
    >
      Open {ticketId} via url
    </button>
  );
}

function renderBoard(entry = '/boards/board-1') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <KeyboardProvider>
        <QueryClientProvider client={client}>
          <ToastProvider>
           <ConfirmProvider>
            <SkipToBoard />
            <LocationProbe />
            <OpenTicketViaUrl ticketId="done-20" />
            <Routes>
              <Route path="/boards/:boardId" element={<BoardPage />} />
              <Route path="*" element={null} />
            </Routes>
           </ConfirmProvider>
          </ToastProvider>
        </QueryClientProvider>
      </KeyboardProvider>
    </MemoryRouter>,
  );
}

function card(name: string, hidden = false) {
  return screen.getByRole('button', { name: new RegExp(`^${name}\\b`), hidden });
}

function press(
  keyName: string,
  target: Document | Element = document,
  init: KeyboardEventInit = {},
) {
  fireEvent.keyDown(target, { key: keyName, ...init });
}

describe('board keyboard navigation', () => {
  beforeEach(() => {
    updateStatus.mockReset();
    updateStatus.mockResolvedValue(undefined);
    createTicket.mockReset();
    createTicket.mockResolvedValue(undefined);
    approveTicketPlan.mockReset();
    approveTicketPlan.mockResolvedValue(undefined);
    mergeBranch.mockReset();
    mergeBranch.mockResolvedValue(undefined);
    gitInfoState.current = null;
    reposState.data = [];
    ticketsState.active = [
      makeTicket({ id: 'alpha', title: 'Alpha', status: 'backlog' }),
      makeTicket({ id: 'beta', title: 'Beta', status: 'backlog' }),
      makeTicket({ id: 'gamma', title: 'Gamma', status: 'ready' }),
    ];
  });

  it('uses one roving tab stop, a focus ring, and the column names', () => {
    renderBoard();

    expect(document.querySelectorAll('[data-board-tabstop="true"]')).toHaveLength(1);
    expect(card('Alpha')).toHaveAttribute('tabindex', '0');
    expect(card('Beta')).toHaveAttribute('tabindex', '-1');
    expect(card('Alpha')).toHaveClass('keyboard-focus');
    expect(screen.getByRole('region', { name: 'Backlog' })).toBeVisible();
    expect(screen.getByRole('region', { name: 'Done' })).toBeVisible();
  });

  it('moves between cards and columns with J/K, H/L, and arrows without wrapping', () => {
    renderBoard();

    press('j');
    expect(document.activeElement).toBe(card('Beta'));
    press('j');
    expect(document.activeElement).toBe(card('Beta'));

    press('k');
    expect(document.activeElement).toBe(card('Alpha'));
    press('ArrowUp');
    expect(document.activeElement).toBe(card('Alpha'));

    press('l');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Plan Review', level: 2 }),
    );
    press('l');
    expect(document.activeElement).toBe(card('Gamma'));
    press('ArrowLeft');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Plan Review', level: 2 }),
    );
    press('h');
    expect(document.activeElement).toBe(card('Alpha'));
    press('h');
    expect(document.activeElement).toBe(card('Alpha'));

    press('j');
    press('ArrowRight');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Plan Review', level: 2 }),
    );
  });

  it('jumps to columns with G then 1–9 and opens the focused card with Enter', () => {
    renderBoard();

    press('g');
    press('3');
    expect(document.activeElement).toBe(card('Gamma'));

    press('g');
    press('5');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'In Review', level: 2 }),
    );

    press('g');
    press('8');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Done', level: 2 }),
    );
    press('l');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Done', level: 2 }),
    );

    press('g');
    press('1');
    card('Alpha').focus();
    press('Enter');
    expect(screen.getByTestId('location')).toHaveTextContent('/boards/board-1?ticket=alpha');
    expect(screen.getByTestId('ticket-drawer')).toBeInTheDocument();
  });

  it('returns focus to the card on Esc and ignores shortcuts while typing', () => {
    renderBoard();

    press('j');
    press('Enter');
    expect(screen.getByTestId('location')).toHaveTextContent('ticket=beta');

    const input = screen.getByPlaceholderText('Add ticket…');
    input.focus();
    press('j', input);
    expect(document.activeElement).toBe(input);
    expect(card('Beta')).toHaveAttribute('data-board-tabstop', 'true');

    press('Escape', input);
    expect(document.activeElement).not.toBe(input);
    expect(screen.getByTestId('location')).toHaveTextContent('ticket=beta');

    press('Escape');
    expect(screen.getByTestId('location')).toHaveTextContent('/boards/board-1');
    expect(screen.queryByTestId('ticket-drawer')).not.toBeInTheDocument();
    expect(document.activeElement).toBe(card('Beta'));
  });

  it('focuses Add ticket… with C and leaves other columns alone', async () => {
    renderBoard();

    press('c');
    const input = screen.getByPlaceholderText('Add ticket…');
    expect(input).toHaveFocus();
    fireEvent.change(input, { target: { value: 'New gate' } });
    await act(async () => {
      fireEvent.submit(input.closest('form')!);
    });
    expect(createTicket).toHaveBeenCalledWith({ title: 'New gate' });

    card('Alpha').focus();
    press('l');
    press('c');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Plan Review', level: 2 }),
    );
  });

  it('does not run Create ticket or help while typing or in the terminal', () => {
    renderBoard();

    const input = screen.getByPlaceholderText('Add ticket…');
    input.focus();
    const typed = new KeyboardEvent('keydown', {
      key: 'c',
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(typed);
    expect(typed.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(input);
    const helpKey = new KeyboardEvent('keydown', {
      key: '?',
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(helpKey);
    expect(helpKey.defaultPrevented).toBe(false);
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();

    const terminal = document.createElement('div');
    terminal.className = 'xterm';
    const termInput = document.createElement('textarea');
    terminal.appendChild(termInput);
    document.body.appendChild(terminal);
    termInput.focus();
    const fromTerminal = new KeyboardEvent('keydown', {
      key: '?',
      bubbles: true,
      cancelable: true,
    });
    termInput.dispatchEvent(fromTerminal);
    expect(fromTerminal.defaultPrevented).toBe(false);
    expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull();
    terminal.remove();
  });

  it('moves the focused card with M then L and keeps focus when the move is rejected', async () => {
    renderBoard();

    press('m');
    press('h');
    expect(updateStatus).not.toHaveBeenCalled();

    press('j');
    press('m');
    press('l');
    expect(updateStatus).toHaveBeenCalledWith({
      ticketId: 'beta',
      status: 'plan_review',
    });
    expect(document.activeElement).toBe(card('Beta'));
    expect(await screen.findByText('Moved to Plan Review')).toBeInTheDocument();

    updateStatus.mockRejectedValueOnce(new Error('rejected'));
    press('m');
    press('l');
    expect(await screen.findByText('Something went wrong.')).toBeInTheDocument();
    expect(screen.getAllByText('Moved to Plan Review')).toHaveLength(1);
  });

  it('opens the drawer to assign and jumps to Knowledge with G then K', () => {
    renderBoard();

    press('a');
    expect(screen.getByTestId('location').textContent).toBe('/boards/board-1?ticket=alpha');

    press('Escape');
    press('g');
    press('k');
    expect(screen.getByTestId('location').textContent).toBe('/knowledge');
  });

  it('skips to the board tab stop', () => {
    renderBoard();

    fireEvent.click(screen.getByRole('link', { name: 'Skip to board' }));
    expect(document.activeElement).toBe(card('Alpha'));
  });

  it('announces a successful move and stays quiet when the move is rejected', async () => {
    updateStatus.mockRejectedValueOnce(new Error('rejected'));
    renderBoard();

    fireEvent.click(screen.getByRole('button', { name: 'Drop alpha on Ready' }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(updateStatus).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Moved to Ready')).not.toBeInTheDocument();

    updateStatus.mockResolvedValueOnce(undefined);
    fireEvent.click(screen.getByRole('button', { name: 'Drop alpha on Ready' }));
    expect(await screen.findByText('Moved to Ready')).toBeInTheDocument();
  });

  it('ignores board movement while Filters or Board settings is open', () => {
    for (const dialog of ['Filters', 'Board settings'] as const) {
      renderBoard();
      press('j');
      fireEvent.click(screen.getByRole('button', { name: dialog }));
      press('j');
      expect(card('Beta', true)).toHaveAttribute('data-board-tabstop', 'true');
      cleanup();
    }
  });
});

function doneTickets(count: number): Ticket[] {
  return Array.from({ length: count }, (_, index) =>
    makeTicket({
      id: `done-${String(index).padStart(2, '0')}`,
      title: `Done ${String(index).padStart(2, '0')}`,
      status: 'done',
      updatedAt: new Date(Date.UTC(2026, 9, 31, 12, 0, 0) - index * 60_000).toISOString(),
    }),
  );
}

function activateButton(button: HTMLElement, key: string) {
  button.focus();
  const event = new KeyboardEvent('keydown', {
    key,
    bubbles: true,
    cancelable: true,
  });
  button.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(false);
  if (!event.defaultPrevented) {
    fireEvent.click(button);
  }
}

describe('board keyboard navigation with a limited Done column', () => {
  beforeEach(() => {
    updateStatus.mockReset();
    updateStatus.mockResolvedValue(undefined);
    createTicket.mockReset();
    createTicket.mockResolvedValue(undefined);
    approveTicketPlan.mockReset();
    mergeBranch.mockReset();
    gitInfoState.current = null;
    reposState.data = [];
    ticketsState.active = doneTickets(32);
  });

  it('stops J/K on the last visible Done card', () => {
    renderBoard();

    press('g');
    press('8');
    expect(document.activeElement).toBe(card('Done 00'));

    for (let step = 0; step < 20; step += 1) {
      press('j');
    }
    expect(document.activeElement).toBe(card('Done 09'));
    expect(screen.queryByText('Done 10')).toBeNull();
  });

  it('reveals more with Enter and Space on the button instead of opening a ticket', () => {
    renderBoard();

    const button = screen.getByRole('button', { name: 'Show 10 more' });
    activateButton(button, 'Enter');
    expect(screen.queryByTestId('ticket-drawer')).not.toBeInTheDocument();
    expect(screen.getByText('Done 10')).toBeVisible();
    expect(screen.getByText('Showing 20 of 32')).toBeVisible();

    activateButton(screen.getByRole('button', { name: 'Show 10 more' }), ' ');
    expect(screen.queryByTestId('ticket-drawer')).not.toBeInTheDocument();
    expect(screen.getByText('Done 20')).toBeVisible();
    expect(screen.getByText('Showing 30 of 32')).toBeVisible();
  });

  it('moves focus to the first newly revealed card when the last click removes the button', () => {
    ticketsState.active = doneTickets(12);
    renderBoard();

    fireEvent.click(screen.getByRole('button', { name: 'Show 2 more' }));
    expect(screen.queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
    expect(document.activeElement).toBe(card('Done 10'));
  });

  it('mounts a hidden Done card and returns focus after closing a ?ticket= drawer', async () => {
    renderBoard('/boards/board-1?ticket=done-20');

    expect(screen.getByTestId('ticket-drawer')).toBeInTheDocument();
    expect(screen.queryByText('Done 20')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Close drawer' }));
    await waitFor(() => {
      expect(screen.queryByTestId('ticket-drawer')).not.toBeInTheDocument();
    });
    expect(screen.getByText('Done 20')).toBeVisible();
    expect(document.activeElement).toBe(card('Done 20'));
  });

  it('keeps a focused Done card mounted after it is pushed past the page limit', () => {
    ticketsState.active = doneTickets(10);
    renderBoard();

    press('g');
    press('8');
    for (let step = 0; step < 9; step += 1) {
      press('j');
    }
    expect(document.activeElement).toBe(card('Done 09'));

    ticketsState.active = [
      makeTicket({
        id: 'done-new',
        title: 'Done new',
        status: 'done',
        updatedAt: '2026-10-31T13:00:00.000Z',
      }),
      ...ticketsState.active,
    ];
    fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    expect(
      screen.getByRole('heading', { name: 'Done', level: 2 }).closest('header'),
    ).toHaveTextContent('11');
    expect(screen.getByText('Done new')).toBeVisible();
    expect(screen.getByText('Done 09')).toBeVisible();
    expect(card('Done 09')).toHaveAttribute('data-board-tabstop', 'true');
  });
});
