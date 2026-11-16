import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { ReactNode } from 'react';
import { act, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

const apiFetchMock = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: (...args: unknown[]) => apiFetchMock(...args),
}));

import { useRemoveWorktree } from './useTicket';

const TICKET_ID = '00000000-0000-0000-0000-000000000001';

function wrapperFor(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

describe('useRemoveWorktree', () => {
  beforeEach(() => {
    apiFetchMock.mockReset();
  });

  it('invalidates the ticket and board list after a successful remove', async () => {
    apiFetchMock.mockResolvedValue({
      json: async () => ({
        ticketBranch: 'agent/TICKET-00000000',
        worktreePath: '/tmp/gone',
        worktreeExists: false,
        defaultBranch: 'main',
        branches: ['main'],
        gitState: 'merged',
      }),
    });
    const client = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useRemoveWorktree(TICKET_ID), {
      wrapper: wrapperFor(client),
    });

    await act(async () => {
      await result.current.mutateAsync();
    });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['ticket', TICKET_ID] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['tickets'] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['comments', TICKET_ID] });
  });
});
