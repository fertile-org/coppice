import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import { BoardSwitcher } from './BoardSwitcher';
import type { Board } from './useBoards';

const boardsState = vi.hoisted(() => ({
  data: [] as Board[],
  isPending: false,
}));

const setLastBoardId = vi.hoisted(() => vi.fn());
const createBoard = vi.hoisted(() =>
  vi.fn().mockResolvedValue({
    id: 'new-board',
    name: 'New',
    slug: 'new',
    createdAt: '2026-01-01T00:00:00.000Z',
  }),
);

vi.mock('./useBoards', () => ({
  setLastBoardId,
  useBoards: () => boardsState,
  useCreateBoard: () => ({ mutateAsync: createBoard, isPending: false }),
}));

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderSwitcher(boardId = 'board-1') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[`/boards/${boardId}`]}>
      <KeyboardProvider>
        <QueryClientProvider client={client}>
          <LocationProbe />
          <Routes>
            <Route
              path="/boards/:boardId"
              element={
                <BoardSwitcher
                  boardId={boardId}
                  boardName={
                    boardsState.data.find((b) => b.id === boardId)?.name
                  }
                  boardsPending={boardsState.isPending}
                />
              }
            />
            <Route path="/boards" element={<p>Boards list</p>} />
          </Routes>
        </QueryClientProvider>
      </KeyboardProvider>
    </MemoryRouter>,
  );
}

describe('BoardSwitcher', () => {
  beforeEach(() => {
    setLastBoardId.mockClear();
    createBoard.mockClear();
    boardsState.isPending = false;
    boardsState.data = [
      {
        id: 'board-1',
        name: 'Launch plan',
        slug: 'launch',
        createdAt: '2026-01-01T00:00:00.000Z',
      },
      {
        id: 'board-2',
        name: 'Ops',
        slug: 'ops',
        createdAt: '2026-01-02T00:00:00.000Z',
      },
    ];
  });

  it('lists boards with the current one marked, All boards, and New board', async () => {
    renderSwitcher();
    fireEvent.click(
      screen.getByRole('button', { name: /Switch board, current: Launch plan/i }),
    );
    const menu = await screen.findByRole('dialog', { name: 'Boards' });
    expect(within(menu).getByRole('button', { name: /Launch plan/i })).toBeInTheDocument();
    expect(within(menu).getByRole('button', { name: /Ops/i })).toBeInTheDocument();
    expect(within(menu).getByRole('link', { name: 'All boards' })).toHaveAttribute(
      'href',
      '/boards',
    );
    expect(within(menu).getByRole('button', { name: '+ New board' })).toBeInTheDocument();
  });

  it('navigates to another board and records last-board', async () => {
    renderSwitcher();
    fireEvent.click(
      screen.getByRole('button', { name: /Switch board, current: Launch plan/i }),
    );
    const menu = await screen.findByRole('dialog', { name: 'Boards' });
    fireEvent.click(within(menu).getByRole('button', { name: /Ops/i }));
    expect(setLastBoardId).toHaveBeenCalledWith('board-2');
    expect(screen.getByTestId('location')).toHaveTextContent('/boards/board-2');
  });

  it('opens the shared New board dialog from the menu', async () => {
    renderSwitcher();
    fireEvent.click(
      screen.getByRole('button', { name: /Switch board, current: Launch plan/i }),
    );
    const menu = await screen.findByRole('dialog', { name: 'Boards' });
    fireEvent.click(within(menu).getByRole('button', { name: '+ New board' }));
    expect(screen.getByRole('heading', { name: 'New board' })).toBeInTheDocument();
    expect(screen.getByLabelText('Name')).toBeInTheDocument();
  });

  it('closes on Escape and does not move board focus with j while open', async () => {
    renderSwitcher();
    const trigger = screen.getByRole('button', {
      name: /Switch board, current: Launch plan/i,
    });
    fireEvent.click(trigger);
    const menu = await screen.findByRole('dialog', { name: 'Boards' });
    const ops = within(menu).getByRole('button', { name: /Ops/i });
    ops.focus();
    fireEvent.keyDown(ops, { key: 'j' });
    expect(menu).toBeInTheDocument();
    expect(document.activeElement).toBe(ops);
    fireEvent.keyDown(ops, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: 'Boards' })).not.toBeInTheDocument();
  });
});
