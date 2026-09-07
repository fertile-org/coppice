import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChatMessage } from '../../lib/schemas/chat';
import { ChatMessageList, ThinkingIndicator } from './ChatMessageList';

const messages: ChatMessage[] = Array.from({ length: 40 }, (_, index) => ({
  id: `00000000-0000-4000-8000-${String(index).padStart(12, '0')}`,
  sessionId: '00000000-0000-4000-8000-000000000099',
  seq: index + 1,
  role: index % 2 === 0 ? 'human' : 'agent',
  body: index % 2 === 0 ? `Human turn ${index}` : `**Agent** reply ${index}`,
  agentRunId: null,
  actionMetadata: null,
  createdAt: '2026-09-08T00:00:00Z',
}));

describe('ChatMessageList', () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
      configurable: true,
      get() {
        return 480;
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
      configurable: true,
      get() {
        return 640;
      },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows a thinking indicator while awaiting a reply', () => {
    render(<ThinkingIndicator />);
    expect(screen.getByTestId('thinking-indicator')).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByRole('status')).toHaveTextContent('Thinking…');
  });

  it('virtualizes a long transcript in a scrollable log', () => {
    render(<ChatMessageList messages={messages} thinking />);

    const list = screen.getByTestId('chat-message-list');
    expect(list).toHaveAttribute('role', 'log');
    expect(screen.getByTestId('thinking-indicator')).toBeInTheDocument();

    const renderedRows = list.querySelectorAll('[data-index]');
    expect(renderedRows.length).toBeGreaterThan(0);
    expect(renderedRows.length).toBeLessThan(messages.length);
    expect(screen.queryByText('Human turn 0')).toBeInTheDocument();
    expect(screen.queryByText('Human turn 39')).not.toBeInTheDocument();
  });
});
