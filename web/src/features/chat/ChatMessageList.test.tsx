import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactElement } from 'react';
import type { ChatMessage } from '../../lib/schemas/chat';
import { ChatMessageList, ThinkingIndicator } from './ChatMessageList';

vi.mock('../tickets/useOpenTicket', () => ({
  useOpenTicket: () => vi.fn(),
}));

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

function renderList(ui: ReactElement) {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

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
    renderList(<ChatMessageList messages={messages} thinking />);

    const list = screen.getByTestId('chat-message-list');
    expect(list).toHaveAttribute('role', 'log');
    expect(screen.getByTestId('thinking-indicator')).toBeInTheDocument();

    const renderedRows = list.querySelectorAll('[data-index]');
    expect(renderedRows.length).toBeGreaterThan(0);
    expect(renderedRows.length).toBeLessThan(messages.length);
    expect(screen.queryByText('Human turn 0')).toBeInTheDocument();
    expect(screen.queryByText('Human turn 39')).not.toBeInTheDocument();
  });

  it('sizes rows from measured content so tall messages do not clip to 88px', () => {
    const tallBody = Array.from({ length: 12 }, (_, i) => `Paragraph ${i}.`).join(
      '\n\n',
    );
    const tallMessages: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'agent',
        body: tallBody,
        agentRunId: null,
        actionMetadata: null,
        createdAt: '2026-09-08T00:00:00Z',
      },
      {
        id: '00000000-0000-4000-8000-000000000002',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 2,
        role: 'human',
        body: 'Short follow-up',
        agentRunId: null,
        actionMetadata: null,
        createdAt: '2026-09-08T00:00:01Z',
      },
    ];

    const TALL_ROW_HEIGHT = 240;
    const SHORT_ROW_HEIGHT = 64;
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(
      function getBoundingClientRect(this: HTMLElement) {
        const indexAttr = this.getAttribute?.('data-index');
        const height =
          indexAttr === '0'
            ? TALL_ROW_HEIGHT
            : indexAttr === '1'
              ? SHORT_ROW_HEIGHT
              : 0;
        return {
          x: 0,
          y: 0,
          top: 0,
          left: 0,
          bottom: height,
          right: 640,
          width: 640,
          height,
          toJSON() {
            return {};
          },
        };
      },
    );

    renderList(<ChatMessageList messages={tallMessages} />);

    const list = screen.getByTestId('chat-message-list');
    const tallRow = list.querySelector('[data-index="0"]');
    expect(tallRow).not.toBeNull();
    // Content-driven rows must not lock height to the 88px estimate.
    expect(tallRow).not.toHaveStyle({ height: '88px' });

    const spacer = list.firstElementChild as HTMLElement | null;
    expect(spacer).not.toBeNull();
    expect(Number.parseFloat(spacer!.style.height)).toBeGreaterThanOrEqual(
      TALL_ROW_HEIGHT + SHORT_ROW_HEIGHT,
    );
  });

  it('surfaces action result chips for ticket, knowledge, and cutoff', () => {
    const actionMessages: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'system',
        body: 'Created ticket',
        agentRunId: null,
        actionMetadata: {
          action: 'create_ticket',
          ticketId: '00000000-0000-4000-8000-000000000050',
        },
        createdAt: '2026-09-08T00:00:00Z',
      },
      {
        id: '00000000-0000-4000-8000-000000000002',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 2,
        role: 'system',
        body: 'Proposed knowledge',
        agentRunId: null,
        actionMetadata: {
          action: 'create_knowledge',
          knowledgeItemId: '00000000-0000-4000-8000-000000000060',
        },
        createdAt: '2026-09-08T00:00:01Z',
      },
      {
        id: '00000000-0000-4000-8000-000000000003',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 3,
        role: 'system',
        body: 'Session cutoff',
        agentRunId: null,
        actionMetadata: {
          action: 'cutoff',
          childSessionId: '00000000-0000-4000-8000-000000000070',
        },
        createdAt: '2026-09-08T00:00:02Z',
      },
    ];

    renderList(<ChatMessageList messages={actionMessages} />);

    expect(screen.getByTestId('chat-action-chip-ticket')).toBeInTheDocument();
    expect(screen.getByTestId('chat-action-chip-knowledge')).toHaveAttribute(
      'href',
      '/knowledge',
    );
    expect(screen.getByTestId('chat-action-chip-cutoff')).toHaveAttribute(
      'href',
      '/chat/00000000-0000-4000-8000-000000000070',
    );
  });
});
