import { describe, expect, it, vi, beforeEach } from 'vitest';

const invalidateSpy = vi.fn();
const setQueryDataSpy = vi.fn();

vi.mock('../../lib/query-client', () => ({
  queryClient: {
    invalidateQueries: (...args: unknown[]) => invalidateSpy(...args),
    setQueryData: (...args: unknown[]) => setQueryDataSpy(...args),
  },
}));

describe('useEventSocket dispatch', () => {
  beforeEach(() => {
    invalidateSpy.mockClear();
    setQueryDataSpy.mockClear();
  });

  it('invalidates ticket and tickets queries on ticket.updated', async () => {
    const { dispatchMessageForTest } = await import('./useEventSocket');
    dispatchMessageForTest(
      JSON.stringify({
        type: 'ticket.updated',
        ticket_id: 'ticket-123',
        status: 'in_review',
      }),
    );

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['tickets'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['ticket', 'ticket-123'] });
  });

  it('invalidates agent runs on agent_run.started', async () => {
    const { dispatchMessageForTest } = await import('./useEventSocket');
    dispatchMessageForTest(
      JSON.stringify({
        type: 'agent_run.started',
        run_id: 'run-1',
        ticket_id: 'ticket-456',
        agent_id: 'agent-1',
        status: 'running',
      }),
    );

    expect(setQueryDataSpy).toHaveBeenCalledWith(
      ['agent-runs', 'ticket-456'],
      expect.any(Function),
    );
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ['agent-runs', 'ticket-456'],
    });
  });

  it('invalidates all queries on resync (receiver lag reconciliation)', async () => {
    const { dispatchMessageForTest } = await import('./useEventSocket');
    dispatchMessageForTest(JSON.stringify({ type: 'resync' }));

    expect(invalidateSpy).toHaveBeenCalledWith();
  });
});
