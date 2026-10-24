import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { ReactNode } from 'react';
import { act, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ApiError } from '../../lib/api';

const apiFetchMock = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: (...args: unknown[]) => apiFetchMock(...args),
}));

import { useRunAgent } from './useAgentRuns';
import { useAssignAgent } from './useTicket';

const TICKET_ID = '00000000-0000-0000-0000-000000000001';

function wrapperFor(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

function newClient() {
  return new QueryClient({ defaultOptions: { mutations: { retry: false } } });
}

describe('run-starting mutations refresh the runs list', () => {
  beforeEach(() => {
    apiFetchMock.mockReset();
  });

  it('refetches runs, ticket and board after an active-run conflict', async () => {
    apiFetchMock.mockRejectedValue(
      new ApiError(409, JSON.stringify({ message: 'An active run already exists for this ticket.' })),
    );
    const client = newClient();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useRunAgent(TICKET_ID), {
      wrapper: wrapperFor(client),
    });

    await act(async () => {
      await result.current.mutateAsync().catch(() => undefined);
    });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['agent-runs', TICKET_ID] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['ticket', TICKET_ID] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['tickets'] });
  });

  it('leaves the cache alone for other run-agent errors', async () => {
    apiFetchMock.mockRejectedValue(new ApiError(400, JSON.stringify({ message: 'nope' })));
    const client = newClient();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useRunAgent(TICKET_ID), {
      wrapper: wrapperFor(client),
    });

    await act(async () => {
      await result.current.mutateAsync().catch(() => undefined);
    });

    expect(invalidate).not.toHaveBeenCalled();
  });

  it('refetches runs after assigning, since auto-start may queue one', async () => {
    apiFetchMock.mockResolvedValue({
      json: async () => ({
        id: TICKET_ID,
        boardId: '00000000-0000-0000-0000-000000000002',
        hasActiveRun: true,
      }),
    });
    const client = newClient();
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const { result } = renderHook(() => useAssignAgent(TICKET_ID), {
      wrapper: wrapperFor(client),
    });

    await act(async () => {
      await result.current.mutateAsync('00000000-0000-0000-0000-000000000010');
    });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['agent-runs', TICKET_ID] });
  });
});
