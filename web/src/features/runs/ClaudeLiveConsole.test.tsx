import '@testing-library/jest-dom/vitest';
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ClaudeLiveConsole } from './ClaudeLiveConsole';

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close = vi.fn(() => {
    this.onclose?.();
  });

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }
}

describe('ClaudeLiveConsole reconnect guard', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('reconnects a stale terminal cache entry while the selected run still needs reconciliation', async () => {
    render(
      <ClaudeLiveConsole
        runId="run-1"
        runStatus="succeeded"
        shouldReconnect
        startedAt="2026-01-01T00:00:00Z"
      />,
    );

    expect(MockWebSocket.instances).toHaveLength(1);

    act(() => {
      MockWebSocket.instances[0].close();
    });
    expect(screen.getByRole('status')).toHaveTextContent('Reconnecting');

    await act(async () => {});
    act(() => {
      vi.advanceTimersByTime(800);
    });

    expect(MockWebSocket.instances).toHaveLength(2);
  });
});
