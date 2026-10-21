import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { Ticket } from '../board/useTickets';
import { useTicket } from './useTicket';

const fetchState = vi.hoisted(() => ({
  resolve: null as ((ticket: Ticket) => void) | null,
}));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: () =>
    new Promise((resolve) => {
      fetchState.resolve = (ticket) => resolve({ json: async () => ticket });
    }),
}));

const listTicket: Ticket = {
  id: 'ticket-1',
  boardId: 'board-1',
  title: 'From the board list',
  status: 'in_progress',
  createdBy: 'user',
  createdAt: '2026-10-21T00:00:00.000Z',
  updatedAt: '2026-10-21T00:00:00.000Z',
  lastActivityAt: '2026-10-21T00:00:00.000Z',
};

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(['tickets', 'board-1', false], [listTicket]);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return wrapper;
}

describe('useTicket list placeholder', () => {
  it('shows the board-list ticket until the detail fetch resolves', async () => {
    const wrapper = setup();
    const { result } = renderHook(
      () => useTicket('ticket-1', { placeholderFromList: true }),
      { wrapper },
    );

    expect(result.current.isPlaceholderData).toBe(true);
    expect(result.current.isLoading).toBe(false);
    expect(result.current.data?.title).toBe('From the board list');
    expect(result.current.data?.description).toBeUndefined();

    await waitFor(() => expect(fetchState.resolve).not.toBeNull());
    fetchState.resolve!({ ...listTicket, description: 'Full description' });

    await waitFor(() => expect(result.current.isPlaceholderData).toBe(false));
    expect(result.current.data?.description).toBe('Full description');
  });

  it('has no placeholder unless asked', () => {
    const wrapper = setup();
    const { result } = renderHook(() => useTicket('ticket-1'), { wrapper });

    expect(result.current.isPlaceholderData).toBe(false);
    expect(result.current.isLoading).toBe(true);
    expect(result.current.data).toBeUndefined();
  });
});
