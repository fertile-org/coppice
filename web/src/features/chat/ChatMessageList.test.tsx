import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactElement } from 'react';
import type { ChatMessage } from '../../lib/schemas/chat';
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

  it('shows a thinking indicator while awaiting a reply', () => {
    render(<ThinkingIndicator />);
    expect(screen.getByTestId('thinking-indicator')).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByRole('status')).toHaveTextContent('Thinking…');
  });

  it('shows a design card on the agent message only', () => {
    const runId = '22222222-2222-4222-8222-222222222222';
    const design = {
      id: '11111111-1111-4111-8111-111111111111',
      title: 'Landing page',
      runId,
      options: [{ id: 'a', title: 'A', entry: 'a/index.html' }],
      chosen: null,
    };
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

  it('aligns human and agent bubbles for a messenger thread', () => {
    const pair: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'human',
        body: 'Hello from me',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:00:00Z',
      },
      {
        id: '00000000-0000-4000-8000-000000000002',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 2,
        role: 'agent',
        body: 'Hello from the agent',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:00:01Z',
      },
    ];

    renderList(<ChatMessageList messages={pair} />);

    const human = screen.getByLabelText('You');
    const agent = screen.getByLabelText('Agent');
    expect(human).toHaveAttribute('data-role', 'human');
    expect(agent).toHaveAttribute('data-role', 'agent');
    expect(human.parentElement).toHaveClass('justify-end');
    expect(agent.parentElement).toHaveClass('justify-start');
    expect(human.className).toMatch(/bg-bark-800/);
    expect(agent.className).toMatch(/bg-moss-50/);
  });

  it('left-aligns system notices like agent replies', () => {
    const notices: ChatMessage[] = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        sessionId: '00000000-0000-4000-8000-000000000099',
        seq: 1,
        role: 'system',
        body: 'Opening coppice…',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:00:00Z',
      },
    ];

    renderList(<ChatMessageList messages={notices} />);

    const system = screen.getByLabelText('System');
    expect(system).toHaveAttribute('data-role', 'system');
    expect(system.parentElement).toHaveClass('justify-start');
    expect(system.parentElement).not.toHaveClass('justify-center');
  });

  it('shows streaming agent row inside the virtualized list', () => {
    renderList(
      <ChatMessageList
        messages={[]}
        streamingAgent={{ runId: 'run-1', text: 'Partial…' }}
      />,
    );
    expect(screen.getByText(/Partial/)).toBeInTheDocument();
    expect(screen.getByTestId('chat-streaming-bubble')).toBeInTheDocument();
  });

  it('shows one thinking bubble, not one inside another, before text arrives', () => {
    renderList(
      <ChatMessageList messages={[]} streamingAgent={{ runId: 'run-1', text: '' }} />,
    );
    const indicator = screen.getByTestId('thinking-indicator');
    expect(indicator.closest('article')).toBeNull();
    expect(screen.getAllByRole('status')).toHaveLength(1);
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
        attachmentIds: [],
        attachments: [],
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
        attachmentIds: [],
        attachments: [],
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
    // Content-driven rows must not lock height to the estimate.
    expect(tallRow).not.toHaveStyle({ height: '72px' });
    expect(tallRow).not.toHaveStyle({ height: '88px' });

    const spacer = list.firstElementChild as HTMLElement | null;
    expect(spacer).not.toBeNull();
    expect(Number.parseFloat(spacer!.style.height)).toBeGreaterThanOrEqual(
      TALL_ROW_HEIGHT + SHORT_ROW_HEIGHT,
    );
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
