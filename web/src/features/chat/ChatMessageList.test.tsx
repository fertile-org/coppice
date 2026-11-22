import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactElement } from 'react';
import type { ChatMessage } from '../../lib/schemas/chat';
import { makeDesignSet, makeOption } from '../designs/designFixtures';
import { ChatMessageBubble, ChatMessageList, ThinkingIndicator } from './ChatMessageList';

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
  attachmentIds: [],
  attachments: [],
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

  it('shows a design card on the agent message only', () => {
    const runId = '22222222-2222-4222-8222-222222222222';
    const design = makeDesignSet({
      runId,
      options: [makeOption('a', 'Quiet', 'A')],
    });
    const shared = {
      sessionId: '00000000-0000-4000-8000-000000000099',
      agentRunId: runId,
      actionMetadata: null,
      attachmentIds: [] as string[],
      attachments: [],
      createdAt: '2026-09-08T00:00:00Z',
    };
    renderList(
      <>
        <ChatMessageBubble
          message={{
            ...shared,
            id: '00000000-0000-4000-8000-000000000001',
            seq: 1,
            role: 'human',
            body: 'Hello from me',
          }}
          designs={[design]}
        />
        <ChatMessageBubble
          message={{
            ...shared,
            id: '00000000-0000-4000-8000-000000000002',
            seq: 2,
            role: 'agent',
            body: 'Here are two options',
          }}
          designs={[design]}
        />
      </>,
    );

    const open = screen.getByRole('button', { name: 'Open' });
    expect(screen.getByLabelText('Agent').parentElement).toContainElement(open);
    expect(screen.getByLabelText('You').parentElement).not.toContainElement(open);
  });

  it('keeps proposal cards outside the agent prose size override', () => {
    const sessionId = '00000000-0000-4000-8000-000000000099';
    const boardId = '00000000-0000-4000-8000-000000000010';
    renderList(
      <ChatMessageBubble
        message={{
          id: '00000000-0000-4000-8000-000000000002',
          sessionId,
          seq: 1,
          role: 'agent',
          body: 'Draft ready for you to confirm.',
          agentRunId: null,
          actionMetadata: {
            proposals: [
              {
                id: '00000000-0000-4000-8000-000000000071',
                type: 'create_ticket',
                status: 'pending',
                title: 'Harden chat cwd resolution',
                description: 'Unbound chat sessions need a safe cwd.',
                boardId,
              },
            ],
          },
          attachmentIds: [],
          attachments: [],
          createdAt: '2026-09-08T00:00:00Z',
        }}
        proposalContext={{ sessionId, boardId, readOnly: false }}
      />,
    );

    const card = screen.getByTestId('chat-proposal-card');
    const article = screen.getByLabelText('Agent');
    expect(article.contains(card)).toBe(true);
    expect(article.className).not.toMatch(/\[&_p\]/);
    let node = card.parentElement;
    while (node && node !== article) {
      expect(node.className).not.toMatch(/\[&_p\]/);
      node = node.parentElement;
    }
    expect(node).toBe(article);
    expect(article.querySelector('p')?.closest('[class*="[&_p]"]')).not.toBeNull();
  });

  it('opens absolute http links from agent replies in a new tab', () => {
    const reply: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000002',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'agent',
        body: 'See [docs](https://example.com/a.md).',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:00:00Z',
      },
    ];

    renderList(<ChatMessageList messages={reply} />);

    const link = screen.getByRole('link', { name: 'docs' });
    expect(link).toHaveAttribute('href', 'https://example.com/a.md');
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noreferrer');
  });

  it('streaming thinking bubbles stay single, collapsible, and beside reply prose', () => {
    render(<ThinkingIndicator />);
    expect(screen.getByTestId('thinking-indicator')).toHaveAttribute('aria-busy', 'true');

    const client = new QueryClient();
    const { rerender } = render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ChatMessageList
            messages={[]}
            streamingAgent={{
              runId: 'run-1',
              thinkingText: 'Checking updater paths…',
              replyText: '',
            }}
          />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const indicator = screen.getByTestId('thinking-indicator');
    expect(indicator.closest('article')).toBeNull();
    expect(screen.getAllByRole('status')).toHaveLength(1);
    expect(screen.getByTestId('chat-streaming-thinking')).toBeInTheDocument();
    expect(screen.queryByTestId('chat-streaming-reply')).not.toBeInTheDocument();

    rerender(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ChatMessageList
            messages={[]}
            streamingAgent={{
              runId: 'run-1',
              thinkingText: 'Checking updater paths…',
              replyText: 'Signed builds auto-update.',
              streaming: true,
            }}
          />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(screen.getByTestId('chat-streaming-reply')).toHaveTextContent(
      'Signed builds auto-update.',
    );
    expect(screen.getByRole('button', { name: /Thinking/ })).toHaveAttribute(
      'aria-expanded',
      'true',
    );

    rerender(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ChatMessageList
            messages={[]}
            streamingAgent={{
              runId: 'run-1',
              thinkingText: 'Checking updater paths…',
              replyText: 'Signed builds auto-update.',
              streaming: false,
            }}
          />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(screen.getByRole('button', { name: /Thinking/ })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    expect(screen.getByTestId('chat-streaming-bubble')).toBeInTheDocument();
  });

  it('virtualizes a long transcript in a scrollable log', () => {
    renderList(<ChatMessageList messages={messages} thinking />);

    const list = screen.getByTestId('chat-message-list');
    expect(list).toHaveAttribute('role', 'log');
    expect(screen.getByTestId('thinking-indicator')).toBeInTheDocument();
    expect(list.querySelector('[data-index] > .max-w-\\[720px\\]')).toBeTruthy();

    const renderedRows = list.querySelectorAll('[data-index]');
    expect(renderedRows.length).toBeGreaterThan(0);
    expect(renderedRows.length).toBeLessThan(messages.length);
    expect(screen.queryByText('Human turn 0')).toBeInTheDocument();
    expect(screen.queryByText('Human turn 39')).not.toBeInTheDocument();
  });

  it('surfaces action result chips for ticket and knowledge', () => {
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
        attachmentIds: [],
        attachments: [],
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
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:00:01Z',
      },
    ];

    renderList(<ChatMessageList messages={actionMessages} />);

    expect(screen.getByTestId('chat-action-chip-ticket')).toBeInTheDocument();
    expect(screen.getByTestId('chat-action-chip-knowledge')).toHaveAttribute(
      'href',
      '/knowledge',
    );
  });

  it('renders image preview and file attachment in the transcript bubble', () => {
    const attachmentId = '00000000-0000-4000-8000-000000000080';
    const fileId = '00000000-0000-4000-8000-000000000081';
    const withAttachments: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'human',
        body: 'See attached',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [attachmentId, fileId],
        attachments: [
          {
            id: attachmentId,
            filename: 'diagram.png',
            contentType: 'image/png',
            sizeBytes: 2048,
          },
          {
            id: fileId,
            filename: 'notes.txt',
            contentType: 'text/plain',
            sizeBytes: 128,
          },
        ],
        createdAt: '2026-09-08T00:00:00Z',
      },
    ];

    renderList(<ChatMessageList messages={withAttachments} />);

    const image = screen.getByRole('img', { name: 'diagram.png' });
    expect(image).toHaveAttribute('src', `/api/attachments/${attachmentId}`);
    expect(screen.getByText('notes.txt')).toBeInTheDocument();
  });
});
