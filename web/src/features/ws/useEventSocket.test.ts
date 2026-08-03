import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

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

  it('invalidates notification queries on notification.changed', async () => {
    const { dispatchMessageForTest } = await import('./useEventSocket');
    dispatchMessageForTest(
      JSON.stringify({
        type: 'notification.changed',
        recipient_user_id: 'user-123',
      }),
    );

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ['notifications'],
    });
  });
});

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close = vi.fn(() => {
    this.onclose?.();
  });
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }
}

describe('useEventSocket lifecycle', () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
    const { resetEventSocketForTest } = await import('./useEventSocket');
    resetEventSocketForTest();
    invalidateSpy.mockClear();
    setQueryDataSpy.mockClear();
  });

  afterEach(async () => {
    const { resetEventSocketForTest } = await import('./useEventSocket');
    resetEventSocketForTest();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('does not disconnect when callback identity changes', async () => {
    const { useEventSocket } = await import('./useEventSocket');
    const firstHandler = vi.fn();
    const secondHandler = vi.fn();

    const { rerender, unmount } = renderHook(
      ({ onRunFinished }) =>
        useEventSocket({ enabled: true, onRunFinished }),
      { initialProps: { onRunFinished: firstHandler } },
    );

    expect(MockWebSocket.instances).toHaveLength(1);
    rerender({ onRunFinished: secondHandler });

    expect(MockWebSocket.instances).toHaveLength(1);
    expect(MockWebSocket.instances[0].close).not.toHaveBeenCalled();

    unmount();
  });

  it('reconnects with exponential backoff capped by active subscribers', async () => {
    const { useEventSocket } = await import('./useEventSocket');
    renderHook(() => useEventSocket({ enabled: true }));

    act(() => MockWebSocket.instances[0].close());
    act(() => vi.advanceTimersByTime(999));
    expect(MockWebSocket.instances).toHaveLength(1);

    act(() => vi.advanceTimersByTime(1));
    expect(MockWebSocket.instances).toHaveLength(2);

    act(() => MockWebSocket.instances[1].close());
    act(() => vi.advanceTimersByTime(1999));
    expect(MockWebSocket.instances).toHaveLength(2);

    act(() => vi.advanceTimersByTime(1));
    expect(MockWebSocket.instances).toHaveLength(3);
  });

  it('reconnects immediately and refetches realtime queries on tab refocus', async () => {
    const { useEventSocket } = await import('./useEventSocket');
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'visible',
    });
    renderHook(() => useEventSocket({ enabled: true }));

    act(() => MockWebSocket.instances[0].close());
    expect(MockWebSocket.instances).toHaveLength(1);

    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    expect(MockWebSocket.instances).toHaveLength(2);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['agent-runs'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['tickets'] });
  });
});
