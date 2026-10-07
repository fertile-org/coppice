import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { ApiError } from '../../lib/api';
import { BoardPage } from './BoardPage';
import type { Ticket } from './useTickets';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

vi.mock('@dnd-kit/core', () => ({
  DndContext: ({
    children,
    onDragEnd,
  }: {
    children: React.ReactNode;
    onDragEnd: (event: {
      active: { id: string };
      over: { id: string } | null;
    }) => void;
  }) => (
    <div>
      <button
        type="button"
        onClick={() =>
          onDragEnd({
            active: { id: 'ticket-1' },
            over: { id: 'done' },
          })
        }
      >
        Drop on Done
      </button>
      {children}
    </div>
  ),
  DragOverlay: ({ children }: { children: React.ReactNode }) => (
    <div>{children}</div>
  ),
  PointerSensor: function PointerSensor() {},
  useSensor: () => ({}),
  useSensors: () => [],
  useDroppable: () => ({ setNodeRef: vi.fn(), isOver: false }),
  useDraggable: () => ({
    attributes: {},
    listeners: {},
    setNodeRef: vi.fn(),
    transform: null,
    isDragging: false,
  }),
}));

const ticket: Ticket = {
  id: 'ticket-1',
  boardId: 'board-1',
  title: 'Ship the patch',
  description: '',
  status: 'in_progress',
  createdBy: 'user',
  createdAt: '2026-08-03T00:00:00.000Z',
  updatedAt: '2026-08-03T00:00:00.000Z',
  lastActivityAt: '2026-08-03T00:00:00.000Z',
};

function jsonResponse(body: unknown) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('BoardPage drop on Done', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockImplementation(async (path: string, init?: RequestInit) => {
      if (init?.method === 'PATCH' && String(path).endsWith('/status')) {
        throw new ApiError(
          400,
          JSON.stringify({
            message: 'A ticket moves to Done only when you accept it.',
          }),
        );
      }
      if (String(path).includes('/tickets')) {
        return jsonResponse([ticket]);
      }
      if (String(path) === '/api/agents') {
        return jsonResponse({ items: [] });
      }
      return jsonResponse({});
    });
  });

  it('snaps the card back and shows the server message', async () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    render(
      <MemoryRouter initialEntries={['/boards/board-1']}>
        <QueryClientProvider client={client}>
          <ToastProvider>
            <Routes>
              <Route path="/boards/:boardId" element={<BoardPage />} />
            </Routes>
          </ToastProvider>
        </QueryClientProvider>
      </MemoryRouter>,
    );

    const inProgress = await screen.findByRole('region', { name: 'In Progress' });
    expect(within(inProgress).getByText('Ship the patch')).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Drop on Done' }));

    expect(
      await screen.findByText('A ticket moves to Done only when you accept it.'),
    ).toBeVisible();
    await waitFor(() => {
      expect(
        within(screen.getByRole('region', { name: 'In Progress' })).getByText(
          'Ship the patch',
        ),
      ).toBeVisible();
    });
    expect(
      within(screen.getByRole('region', { name: 'Done' })).queryByText(
        'Ship the patch',
      ),
    ).toBeNull();
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/tickets/ticket-1/status',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ status: 'done' }),
      }),
    );
  });
});
