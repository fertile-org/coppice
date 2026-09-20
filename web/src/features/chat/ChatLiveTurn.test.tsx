import '@testing-library/jest-dom/vitest';
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatLiveTurn } from './ChatLiveTurn';

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

function emit(ws: MockWebSocket, payload: unknown) {
  act(() => {
    ws.onmessage?.(
      new MessageEvent('message', { data: JSON.stringify(payload) }),
    );
  });
}

describe('ChatLiveTurn', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('keeps thinking indicator for cursor.console.session without a dark empty shell', () => {
    render(<ChatLiveTurn runId="run-1" runStatus="running" />);
    const ws = MockWebSocket.instances[0]!;
    act(() => {
      ws.onopen?.();
    });

    emit(ws, {
      type: 'event',
      event: { type: 'cursor.console.session', model: 'auto' },
    });

    expect(screen.getByTestId('thinking-indicator')).toBeInTheDocument();
    expect(screen.queryByTestId('chat-console-preview')).not.toBeInTheDocument();
    expect(screen.queryByTestId('chat-opencode-stream')).not.toBeInTheDocument();
    expect(screen.getByTestId('chat-live-turn')).not.toHaveClass('oc-session');
  });

  it('renders cursor.console.text in light chat chrome', () => {
    render(<ChatLiveTurn runId="run-2" runStatus="running" />);
    const ws = MockWebSocket.instances[0]!;
    act(() => {
      ws.onopen?.();
    });

    emit(ws, {
      type: 'event',
      event: {
        type: 'cursor.console.text',
        markdown: 'Here is a draft reply.',
      },
    });

    expect(screen.queryByTestId('thinking-indicator')).not.toBeInTheDocument();
    expect(screen.getByTestId('chat-console-preview')).toHaveTextContent(
      'Here is a draft reply.',
    );
    expect(screen.queryByTestId('chat-opencode-stream')).not.toBeInTheDocument();
  });

  it('calls onFinished when the live stream ends', () => {
    const onFinished = vi.fn();
    render(
      <ChatLiveTurn runId="run-3" runStatus="running" onFinished={onFinished} />,
    );
    const ws = MockWebSocket.instances[0]!;
    act(() => {
      ws.onopen?.();
    });

    emit(ws, {
      type: 'event',
      event: {
        type: 'cursor.console.text',
        markdown: 'Done.',
      },
    });
    emit(ws, { type: 'end', status: 'succeeded' });

    expect(onFinished).toHaveBeenCalledTimes(1);
  });
});
