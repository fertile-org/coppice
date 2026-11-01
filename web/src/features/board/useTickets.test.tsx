import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import { act, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Ticket } from './useTickets';

const apiFetchMock = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: (...args: unknown[]) => apiFetchMock(...args),
}));

import { ticketsQueryKey, useUpdateTicketStatus } from './useTickets';

const BOARD_ID = 'board-1';
const NOW = '2026-10-31T15:00:00.000Z';

function ticket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: 'ticket-1',
    boardId: BOARD_ID,
    title: 'Older done work',
    status: 'in_progress',
    createdBy: 'user',
    createdAt: '2026-08-03T00:00:00.000Z',
    updatedAt: '2026-08-03T00:00:00.000Z',
    lastActivityAt: '2026-08-03T00:00:00.000Z',
    ...overrides,
  };
}

function wrapperFor(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

function newClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

describe('useUpdateTicketStatus optimistic updatedAt', () => {
  beforeEach(() => {
    apiFetchMock.mockReset();
    vi.useFakeTimers();
    vi.setSystemTime(new Date(NOW));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('bumps updatedAt on mutate so a drop into Done sorts to the top before refetch', async () => {
    apiFetchMock.mockResolvedValue({
      json: async () => ticket({ status: 'done', updatedAt: NOW }),
    });
    const client = newClient();
    const listKey = [...ticketsQueryKey(BOARD_ID), false] as const;
    client.setQueryData<Ticket[]>(listKey, [ticket()]);

    const { result } = renderHook(() => useUpdateTicketStatus(BOARD_ID, false), {
      wrapper: wrapperFor(client),
    });

    await act(async () => {
      result.current.mutate({ ticketId: 'ticket-1', status: 'done' });
    });

    expect(client.getQueryData<Ticket[]>(listKey)?.[0]).toMatchObject({
      status: 'done',
      updatedAt: NOW,
    });
  });
});
