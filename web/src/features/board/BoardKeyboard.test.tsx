import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { SkipToBoard } from '../keyboard/SkipToBoard';
import { BoardPage } from './BoardPage';
import type { Ticket } from './useTickets';

vi.mock('./ReviewCapacityBar', () => ({
  ReviewCapacityBar: () => null,
}));

const ticketsState = vi.hoisted(() => ({
  active: [] as Ticket[],
}));

const updateStatus = vi.hoisted(() => vi.fn());

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
  useCreateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useStartPlanRun: () => ({
    mutateAsync: vi.fn(),
    isPending: false,
    variables: undefined,
  }),
  useUpdateTicketStatus: () => ({ mutateAsync: updateStatus }),
}));

vi.mock('../boards/useBoards', () => ({
  setLastBoardId: vi.fn(),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: [] }),
}));

vi.mock('../tickets/TicketDrawer', () => ({
  TicketDrawer: ({
    open = true,
    onClose,
  }: {
    open?: boolean;
    onClose: () => void;
  }) =>
    open ? (
      <div role="dialog" data-shortcut-layer="ticket" data-testid="ticket-drawer">
        <button type="button" onClick={onClose}>
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

function renderBoard() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={['/boards/board-1']}>
      <KeyboardProvider>
        <QueryClientProvider client={client}>
          <ToastProvider>
            <SkipToBoard />
            <LocationProbe />
            <Routes>
              <Route path="/boards/:boardId" element={<BoardPage />} />
            </Routes>
          </ToastProvider>
        </QueryClientProvider>
      </KeyboardProvider>
    </MemoryRouter>,
  );
}

function card(name: string, hidden = false) {
  return screen.getByRole('button', { name: new RegExp(`^${name}\\b`), hidden });
}

function press(keyName: string, target: Document | Element = document) {
  fireEvent.keyDown(target, { key: keyName });
}

describe('board keyboard navigation', () => {
  beforeEach(() => {
    updateStatus.mockReset();
    updateStatus.mockResolvedValue(undefined);
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
    for (const name of [
      'Backlog',
      'Plan Review',
      'Ready',
      'In Progress',
      'In Review',
      'In QA',
      'Wait for Human Review',
      'Done',
      'Blocked',
    ]) {
      expect(screen.getByRole('region', { name })).toBeVisible();
    }
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
    press('9');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Blocked', level: 2 }),
    );
    press('l');
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { name: 'Blocked', level: 2 }),
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

  it('does not run disabled ticket sequences or page jumps as card movement', () => {
    renderBoard();

    press('j');
    expect(document.activeElement).toBe(card('Beta'));
    press('m');
    press('l');
    expect(document.activeElement).toBe(card('Beta'));

    press('g');
    press('k');
    expect(document.activeElement).toBe(card('Beta'));
    expect(screen.getByTestId('location')).toHaveTextContent('/boards/board-1');
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

  it('ignores board movement while the filter dialog is open', () => {
    renderBoard();
    press('j');
    fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    press('j');
    expect(card('Beta', true)).toHaveAttribute('data-board-tabstop', 'true');
  });
});
