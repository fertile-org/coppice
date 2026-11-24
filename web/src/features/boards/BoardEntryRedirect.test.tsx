import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BoardEntryRedirect } from './BoardEntryRedirect';
import type { Board } from './useBoards';

const boardsState = vi.hoisted(() => ({
  data: undefined as Board[] | undefined,
  isPending: true,
  isError: false,
}));

const setLastBoardId = vi.hoisted(() => vi.fn());

vi.mock('./useBoards', () => ({
  setLastBoardId,
  useBoards: () => boardsState,
}));

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderEntry(entry = '/') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <QueryClientProvider client={client}>
        <LocationProbe />
        <Routes>
          <Route path="/" element={<BoardEntryRedirect />} />
          <Route path="/boards" element={<p>Boards list</p>} />
          <Route path="/boards/:boardId" element={<p>Board page</p>} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('BoardEntryRedirect', () => {
  beforeEach(() => {
    setLastBoardId.mockClear();
    boardsState.data = undefined;
    boardsState.isPending = true;
    boardsState.isError = false;
  });

  it('waits for boards without flashing the list', () => {
    renderEntry();
    expect(screen.getByText('Loading boards…')).toBeInTheDocument();
    expect(screen.queryByText('Boards list')).not.toBeInTheDocument();
    expect(screen.getByTestId('location')).toHaveTextContent('/');
  });

  it('opens the only board and records last-board', async () => {
    boardsState.isPending = false;
    boardsState.data = [
      {
        id: 'solo',
        name: 'Solo',
        slug: 'solo',
        createdAt: '2026-01-01T00:00:00.000Z',
      },
    ];
    renderEntry();
    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/boards/solo');
    });
    expect(setLastBoardId).toHaveBeenCalledWith('solo');
    expect(screen.queryByText('Boards list')).not.toBeInTheDocument();
  });

  it('lands on /boards when there are zero boards', async () => {
    boardsState.isPending = false;
    boardsState.data = [];
    renderEntry();
    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/boards');
    });
    expect(setLastBoardId).not.toHaveBeenCalled();
    expect(screen.getByText('Boards list')).toBeInTheDocument();
  });

  it('lands on /boards when there are two or more boards', async () => {
    boardsState.isPending = false;
    boardsState.data = [
      {
        id: 'a',
        name: 'A',
        slug: 'a',
        createdAt: '2026-01-01T00:00:00.000Z',
      },
      {
        id: 'b',
        name: 'B',
        slug: 'b',
        createdAt: '2026-01-01T00:00:00.000Z',
      },
    ];
    renderEntry();
    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/boards');
    });
    expect(setLastBoardId).not.toHaveBeenCalled();
  });

  it('lands on /boards when the boards query errors', async () => {
    boardsState.isPending = false;
    boardsState.isError = true;
    boardsState.data = undefined;
    renderEntry();
    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/boards');
    });
    expect(setLastBoardId).not.toHaveBeenCalled();
  });
});
