import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { ChatPage } from './ChatPage';

const mocks = vi.hoisted(() => ({
  createSession: vi.fn(),
  postMessage: vi.fn(),
  postMessagePending: false,
  uploadAttachment: vi.fn(),
  createTicket: vi.fn(),
  draftTicketData: {
    title: 'Harden chat cwd resolution',
    description: '## Context\n\nUnbound chat sessions need a safe cwd.\n',
    source: 'agent' as 'agent' | 'fallback',
  } as {
    title: string;
    description: string;
    source: 'agent' | 'fallback';
  } | undefined,
  draftTicketLoading: false,
  draftTicketError: false,
  createKnowledge: vi.fn(),
  openTicket: vi.fn(),
  sessions: [] as Array<{
    id: string;
    boardId: string | null;
    ownerUserId: string;
    agentId: string;
    repoId: string | null;
    status: 'active' | 'archived' | 'cutoff';
    lastMessagePreview?: string;
    lastMessageRole?: 'human' | 'agent' | 'system';
    hasActiveRun?: boolean;
    activeRunId?: string | null;
    title?: string | null;
    createdAt: string;
    updatedAt: string;
  }>,
  olderBeforeSeq: null as number | null,
  fetchOlderMessages: vi.fn(),
  messages: [] as Array<{
    id: string;
    sessionId: string;
    seq: number;
    role: 'human' | 'agent' | 'system';
    body: string;
    agentRunId: string | null;
    actionMetadata: unknown;
    attachmentIds: string[];
    attachments: Array<{
      id: string;
      filename: string;
      contentType: string;
      sizeBytes: number;
    }>;
    createdAt: string;
  }>,
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [
      {
        id: '00000000-0000-4000-8000-000000000010',
        name: 'Backend Engineer',
        role: 'Engineer',
        enabled: true,
        connector: 'claude-code',
      },
      {
        id: '00000000-0000-4000-8000-000000000011',
        name: 'Disabled Agent',
        role: 'Engineer',
        enabled: false,
        connector: 'claude-code',
      },
      {
        id: '00000000-0000-4000-8000-000000000012',
        name: 'Kilo Agent',
        role: 'Assistant',
        enabled: true,
        connector: 'kilo-code',
      },
      {
        id: '00000000-0000-4000-8000-000000000013',
        name: 'Codex Agent',
        role: 'Engineer',
        enabled: true,
        connector: 'codex',
      },
      {
        id: '00000000-0000-4000-8000-000000000014',
        name: 'OpenCode Agent',
        role: 'Engineer',
        enabled: true,
        connector: 'opencode',
      },
      {
        id: '00000000-0000-4000-8000-000000000015',
        name: 'Cursor Agent',
        role: 'Assistant',
        enabled: true,
        connector: 'cursor',
      },
    ],
  }),
}));

vi.mock('../boards/useBoards', () => ({
  useBoards: () => ({
    data: [
      {
        id: '00000000-0000-4000-8000-000000000003',
        name: 'Coppice',
        slug: 'coppice',
        createdAt: '2026-09-08T00:00:00Z',
      },
    ],
  }),
}));

vi.mock('../tickets/useOpenTicket', () => ({
  useOpenTicket: () => mocks.openTicket,
}));

vi.mock('../tickets/useTicket', () => ({
  useUploadAttachment: () => ({
    mutateAsync: mocks.uploadAttachment,
    isPending: false,
  }),
}));

vi.mock('./useChat', () => ({
  CHAT_SESSIONS_QUERY_KEY: ['chat-sessions'],
  useChatSessions: () => ({
    data: mocks.sessions,
    isLoading: false,
  }),
  useChatSession: (sessionId: string | null) => ({
    data: mocks.sessions.find((session) => session.id === sessionId),
  }),
  useChatMessages: () => ({
    data: { messages: mocks.messages, olderBeforeSeq: mocks.olderBeforeSeq ?? undefined },
    refetch: vi.fn(),
  }),
  fetchOlderMessages: (...args: unknown[]) => mocks.fetchOlderMessages(...args),
  useCreateChatSession: () => ({
    mutateAsync: mocks.createSession,
    isPending: false,
  }),
  usePostChatMessage: () => ({
    mutateAsync: mocks.postMessage,
    isPending: mocks.postMessagePending,
  }),
  useCreateTicketFromChat: () => ({
    mutateAsync: mocks.createTicket,
    isPending: false,
  }),
  useDraftTicketFromChat: () => ({
    data: mocks.draftTicketData,
    isLoading: mocks.draftTicketLoading,
    isError: mocks.draftTicketError,
  }),
  useCreateKnowledgeFromChat: () => ({
    mutateAsync: mocks.createKnowledge,
    isPending: false,
  }),
}));

vi.mock('./useChatRunLiveStream', () => ({
  useChatRunLiveStream: (runId: string | null) => ({
    text: runId ? 'Partial…' : '',
    connection: 'open' as const,
    error: null,
    finished: false,
    store: null,
    consoleEntries: [],
    awaiting: false,
    thinkingLabel: 'Thinking…',
    hasOpenCode: false,
    hasConsole: false,
  }),
}));

function renderChat(path = '/chat') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/chat" element={<ChatPage />} />
          <Route path="/chat/new/:agentId" element={<ChatPage />} />
          <Route path="/chat/:sessionId" element={<ChatPage />} />
          <Route path="/knowledge" element={<div>Knowledge inbox</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const ACTIVE_SESSION = {
  id: '00000000-0000-4000-8000-000000000001',
  boardId: '00000000-0000-4000-8000-000000000003',
  ownerUserId: '00000000-0000-4000-8000-000000000002',
  agentId: '00000000-0000-4000-8000-000000000010',
  repoId: null,
  status: 'active' as const,
  lastMessagePreview: 'Prior message',
  hasActiveRun: false,
  createdAt: '2026-09-08T00:00:00Z',
  updatedAt: '2026-09-08T00:00:00Z',
};

describe('ChatPage', () => {
  beforeEach(() => {
    mocks.olderBeforeSeq = null;
    mocks.fetchOlderMessages.mockReset();
    mocks.createSession.mockReset();
    mocks.postMessage.mockReset();
    mocks.postMessagePending = false;
    mocks.uploadAttachment.mockReset();
    mocks.createTicket.mockReset();
    mocks.draftTicketData = {
      title: 'Harden chat cwd resolution',
      description: '## Context\n\nUnbound chat sessions need a safe cwd.\n',
      source: 'agent',
    };
    mocks.draftTicketLoading = false;
    mocks.draftTicketError = false;
    mocks.createKnowledge.mockReset();
    mocks.openTicket.mockReset();
    mocks.sessions = [];
    mocks.messages = [];
    vi.stubGlobal(
      'URL',
      class extends URL {
        static createObjectURL = vi.fn(() => 'blob:mock-preview');
        static revokeObjectURL = vi.fn();
      },
    );
  });

  it('lists every enabled agent with name and role', () => {
    renderChat('/chat');

    const list = screen.getByTestId('chat-session-list');
    expect(within(list).getByText('Backend Engineer')).toBeInTheDocument();
    expect(within(list).getAllByText('Engineer').length).toBeGreaterThan(0);
    expect(within(list).getAllByText('No messages yet').length).toBeGreaterThan(0);
    expect(within(list).queryByText('Disabled Agent')).toBeNull();
    expect(screen.queryByTestId('new-chat-form')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Start chat' })).not.toBeInTheDocument();
  });

  it('lists Cursor and Kilo Code as chats that can start', () => {
    renderChat('/chat');

    expect(
      screen.queryByText(
        "Chat isn't available for Kilo Code yet, because Coppice can't keep it read-only.",
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText(
        "Chat isn't available for Cursor yet, because Coppice can't keep it read-only.",
      ),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Kilo Agent/ })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Cursor Agent/ })).toBeInTheDocument();
    expect(screen.queryByTestId('chat-agent-unavailable')).not.toBeInTheDocument();
  });

  it('opens an empty chat and creates the session on the first message', async () => {
    mocks.createSession.mockResolvedValue({
      id: '00000000-0000-4000-8000-000000000001',
      boardId: null,
      ownerUserId: '00000000-0000-4000-8000-000000000002',
      agentId: '00000000-0000-4000-8000-000000000014',
      repoId: null,
      status: 'active',
      createdAt: '2026-09-08T00:00:00Z',
      updatedAt: '2026-09-08T00:00:00Z',
    });
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000031',
        sessionId: '00000000-0000-4000-8000-000000000001',
        seq: 1,
        role: 'human',
        body: 'Hello',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat('/chat');
    fireEvent.click(screen.getByRole('link', { name: /OpenCode Agent/ }));

    expect(
      await screen.findByText(
        'Say hi to OpenCode Agent. They can see all your projects.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByTestId('chat-isolation-banner')).not.toBeInTheDocument();
    expect(mocks.createSession).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('Message'), {
      target: { value: 'Hello' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.createSession).toHaveBeenCalledWith({
        agentId: '00000000-0000-4000-8000-000000000014',
      });
      expect(mocks.postMessage).toHaveBeenCalledWith({
        body: 'Hello',
        sessionId: '00000000-0000-4000-8000-000000000001',
      });
    });
  });

  it('shows a new-chat button whose tooltip names the agent', () => {
    renderChat('/chat');
    const button = screen.getByRole('button', {
      name: 'New chat with Backend Engineer',
    });
    expect(button).toHaveAttribute('title', 'New chat with Backend Engineer');
    expect(button).toHaveTextContent('New chat');
  });

  it('expands an agent that has several chats and titles them from the first message', () => {
    mocks.sessions = [
      {
        ...ACTIVE_SESSION,
        title: 'Fix the login form',
        updatedAt: '2026-09-08T00:02:00Z',
      },
      {
        ...ACTIVE_SESSION,
        id: '00000000-0000-4000-8000-000000000099',
        title: null,
        updatedAt: '2026-09-08T00:01:00Z',
      },
    ];
    renderChat('/chat');

    const list = screen.getByTestId('chat-session-list');
    const backend = within(list)
      .getAllByTestId('chat-agent-row')
      .find((row) => within(row).queryByText('Backend Engineer'));
    expect(backend).toBeTruthy();
    expect(within(backend!).getByRole('link', { name: 'Fix the login form' })).toBeInTheDocument();
    expect(within(backend!).getByRole('link', { name: 'New chat' })).toBeInTheDocument();
    expect(within(backend!).queryByText('No messages yet')).not.toBeInTheDocument();
  });

  it.each([
    [
      '00000000-0000-4000-8000-000000000010',
      "Coppice hasn't confirmed that Claude Code stays read-only. The agent can still run commands on your computer.",
    ],
    [
      '00000000-0000-4000-8000-000000000013',
      "Coppice hasn't confirmed that Codex stays read-only. The agent can still run commands on your computer.",
    ],
    [
      '00000000-0000-4000-8000-000000000015',
      "Coppice hasn't confirmed that Cursor stays read-only. The agent can still run commands on your computer.",
    ],
    [
      '00000000-0000-4000-8000-000000000012',
      "Coppice hasn't confirmed that Kilo Code stays read-only. The agent can still run commands on your computer.",
    ],
  ])('shows the isolation banner and the composer for %s', (agentId, message) => {
    mocks.sessions = [
      {
        ...ACTIVE_SESSION,
        agentId,
      },
    ];
    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    expect(screen.getByTestId('chat-isolation-banner')).toHaveTextContent(message);
    expect(screen.getByTestId('chat-composer')).toBeInTheDocument();
  });

  it('hides the isolation banner for an OpenCode session', () => {
    mocks.sessions = [
      {
        ...ACTIVE_SESSION,
        agentId: '00000000-0000-4000-8000-000000000014',
      },
    ];
    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    expect(screen.queryByTestId('chat-isolation-banner')).not.toBeInTheDocument();
    expect(screen.getByTestId('chat-composer')).toBeInTheDocument();
  });

  it('loads earlier messages above the newest page', async () => {
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
    const message = (seq: number, body: string) => ({
      id: `00000000-0000-4000-8000-${String(100 + seq).padStart(12, '0')}`,
      sessionId: '00000000-0000-4000-8000-000000000001',
      seq,
      role: 'human' as const,
      body,
      agentRunId: null,
      actionMetadata: null,
      attachmentIds: [],
      attachments: [],
      createdAt: '2026-09-08T00:01:00Z',
    });
    mocks.sessions = [ACTIVE_SESSION];
    mocks.messages = [message(100, 'Newest page message')];
    mocks.olderBeforeSeq = 100;
    mocks.fetchOlderMessages.mockResolvedValue({ messages: [message(99, 'An earlier message')] });
    renderChat('/chat/00000000-0000-4000-8000-000000000001');

    fireEvent.click(await screen.findByRole('button', { name: 'Load earlier messages' }));
    expect(await screen.findByText('An earlier message')).toBeInTheDocument();
    expect(mocks.fetchOlderMessages).toHaveBeenCalledWith('00000000-0000-4000-8000-000000000001', 100);
    expect(screen.queryByRole('button', { name: 'Load earlier messages' })).not.toBeInTheDocument();
  });

  it('lists sessions and sends a message that starts a live turn', async () => {
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

    mocks.sessions = [ACTIVE_SESSION];
    mocks.messages = [
      {
        id: '00000000-0000-4000-8000-000000000030',
        sessionId: '00000000-0000-4000-8000-000000000001',
        seq: 1,
        role: 'human',
        body: 'Prior message',
        agentRunId: null,
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:01:00Z',
      },
    ];
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000031',
        sessionId: '00000000-0000-4000-8000-000000000001',
        seq: 2,
        role: 'human',
        body: 'What is cwd?',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat('/chat/00000000-0000-4000-8000-000000000001');

    const transcript = await screen.findByTestId('chat-message-list');
    expect(within(transcript).getByText('Prior message')).toBeInTheDocument();
    expect(screen.getByTestId('chat-session-pane')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Backend Engineer' })).toBeInTheDocument();
    expect(screen.getByTestId('chat-session-actions')).toBeInTheDocument();
    expect(screen.getByTestId('chat-composer')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Message'), {
      target: { value: 'What is cwd?' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.postMessage).toHaveBeenCalledWith({
        body: 'What is cwd?',
      });
    });
    expect(await screen.findByTestId('chat-streaming-bubble')).toBeInTheDocument();
    expect(screen.queryByTestId('chat-live-turn')).toBeNull();
    expect(screen.getByText(/Partial/)).toBeInTheDocument();
  });

  it('fills available height instead of a fixed 70vh/720px pane', () => {
    mocks.sessions = [ACTIVE_SESSION];
    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const page = screen.getByTestId('chat-page');
    expect(page).toHaveClass('flex-1');
    expect(page).toHaveClass('min-h-0');

    const pane = screen.getByTestId('chat-session-pane');
    expect(pane).toHaveClass('flex-1');
    expect(pane).toHaveClass('min-h-0');
    expect(pane.className).not.toMatch(/70vh|720px/);
  });

  it('keeps session list and pane header fixed while transcript scrolls', () => {
    mocks.sessions = [ACTIVE_SESSION];
    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const list = screen.getByTestId('chat-session-list');
    expect(list).toHaveClass('min-h-0');
    expect(list).toHaveClass('overflow-hidden');
    expect(list.querySelector('.overflow-y-auto')).toBeTruthy();
    expect(list).toHaveClass('lg:h-full');

    const grid = screen.getByTestId('chat-layout-grid');
    expect(grid).toHaveClass('min-h-0');
    expect(grid).toHaveClass('flex-1');
    expect(grid).toHaveClass('overflow-hidden');

    const header = screen.getByTestId('chat-session-header');
    expect(header).toHaveClass('shrink-0');

    const pane = screen.getByTestId('chat-session-pane');
    expect(pane).toHaveClass('overflow-hidden');
    expect(pane).toHaveClass('min-h-0');

    const composer = screen.getByTestId('chat-composer');
    expect(composer).toHaveClass('shrink-0');
  });

  it('uploads attachments then posts message with attachmentIds', async () => {
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

    const attachmentId = '00000000-0000-4000-8000-000000000090';
    mocks.sessions = [ACTIVE_SESSION];
    mocks.uploadAttachment.mockResolvedValue({
      id: attachmentId,
      filename: 'shot.png',
      contentType: 'image/png',
      sizeBytes: 1024,
    });
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000031',
        sessionId: ACTIVE_SESSION.id,
        seq: 1,
        role: 'human',
        body: 'Look at this',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: null,
        attachmentIds: [attachmentId],
        attachments: [
          {
            id: attachmentId,
            filename: 'shot.png',
            contentType: 'image/png',
            sizeBytes: 1024,
          },
        ],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const file = new File(['png-bytes'], 'shot.png', { type: 'image/png' });
    fireEvent.change(screen.getByTestId('chat-composer-files'), {
      target: { files: [file] },
    });
    expect(await screen.findByAltText('shot.png')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Message'), {
      target: { value: 'Look at this' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.uploadAttachment).toHaveBeenCalledWith(file);
      expect(mocks.postMessage).toHaveBeenCalledWith({
        body: 'Look at this',
        attachmentIds: [attachmentId],
      });
    });
  });

  it('sends attachment-only messages', async () => {
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

    const attachmentId = '00000000-0000-4000-8000-000000000091';
    mocks.sessions = [ACTIVE_SESSION];
    mocks.uploadAttachment.mockResolvedValue({
      id: attachmentId,
      filename: 'notes.txt',
      contentType: 'text/plain',
      sizeBytes: 12,
    });
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000032',
        sessionId: ACTIVE_SESSION.id,
        seq: 1,
        role: 'human',
        body: '',
        agentRunId: '00000000-0000-4000-8000-000000000041',
        actionMetadata: null,
        attachmentIds: [attachmentId],
        attachments: [
          {
            id: attachmentId,
            filename: 'notes.txt',
            contentType: 'text/plain',
            sizeBytes: 12,
          },
        ],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000041',
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const file = new File(['hello notes'], 'notes.txt', { type: 'text/plain' });
    fireEvent.change(screen.getByTestId('chat-composer-files'), {
      target: { files: [file] },
    });
    expect(await screen.findByText('notes.txt')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.postMessage).toHaveBeenCalledWith({
        body: '',
        attachmentIds: [attachmentId],
      });
    });
  });

  it('surfaces upload errors in the composer', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.uploadAttachment.mockRejectedValue(
      new ApiError(413, JSON.stringify({ message: 'File too large' })),
    );

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const big = new File(['x'], 'big.png', { type: 'image/png' });
    fireEvent.change(screen.getByTestId('chat-composer-files'), {
      target: { files: [big] },
    });
    expect(await screen.findByAltText('big.png')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('File too large');
  });

  it('shows ThinkingIndicator while draft-ticket is loading', () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.draftTicketLoading = true;
    mocks.draftTicketData = undefined;

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    fireEvent.click(screen.getByRole('button', { name: 'Create ticket' }));
    expect(screen.getByTestId('create-ticket-dialog')).toBeInTheDocument();

    const status = screen.getByTestId('draft-ticket-status');
    expect(status).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByTestId('thinking-indicator')).toBeInTheDocument();
    expect(screen.getByTestId('thinking-indicator')).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByTestId('thinking-indicator')).toHaveTextContent(
      /Drafting title and description from this chat/,
    );
    expect(screen.getByLabelText('Title')).toBeDisabled();
    expect(screen.getByLabelText('Description')).toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Confirm create ticket' }),
    ).toBeDisabled();
  });

  it('clears ThinkingIndicator after draft-ticket finishes', async () => {
    mocks.sessions = [ACTIVE_SESSION];

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    fireEvent.click(screen.getByRole('button', { name: 'Create ticket' }));
    expect(screen.getByTestId('create-ticket-dialog')).toBeInTheDocument();
    expect(await screen.findByDisplayValue('Harden chat cwd resolution')).toBeInTheDocument();

    expect(screen.queryByTestId('thinking-indicator')).not.toBeInTheDocument();
    expect(screen.getByTestId('draft-ticket-status')).not.toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(screen.getByTestId('draft-ticket-status')).toHaveTextContent(
      'Review the agent draft, edit if needed, then confirm.',
    );
    expect(screen.getByLabelText('Title')).not.toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Confirm create ticket' }),
    ).not.toBeDisabled();
  });

  it('explains a basic draft when the agent could not draft the ticket', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.draftTicketData = {
      title: 'From the chat',
      description: 'Notes from the conversation.',
      source: 'fallback',
    };

    renderChat(`/chat/${ACTIVE_SESSION.id}`);
    fireEvent.click(screen.getByRole('button', { name: 'Create ticket' }));

    expect(await screen.findByDisplayValue('From the chat')).toBeInTheDocument();
    expect(screen.getByTestId('draft-ticket-status')).toHaveTextContent(
      "The agent couldn't draft this, so here's a basic draft from the chat. Edit it before you confirm.",
    );
    expect(screen.getByLabelText('Title')).not.toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Confirm create ticket' }),
    ).not.toBeDisabled();
  });

  it('creates a ticket from chat after confirming board and title', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.createTicket.mockResolvedValue({
      ticket: {
        id: '00000000-0000-4000-8000-000000000050',
        boardId: '00000000-0000-4000-8000-000000000003',
        title: 'Fix chat cwd',
        status: 'backlog',
      },
      message: {
        id: '00000000-0000-4000-8000-000000000051',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'system',
        body: 'Created ticket',
        agentRunId: null,
        actionMetadata: {
          action: 'create_ticket',
          ticketId: '00000000-0000-4000-8000-000000000050',
        },
        createdAt: '2026-09-08T00:03:00Z',
      },
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    fireEvent.click(screen.getByRole('button', { name: 'Create ticket' }));
    expect(screen.getByTestId('create-ticket-dialog')).toBeInTheDocument();
    expect(await screen.findByDisplayValue('Harden chat cwd resolution')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Title'), {
      target: { value: 'Fix chat cwd' },
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'Confirm create ticket' }),
    );

    await waitFor(() => {
      expect(mocks.createTicket).toHaveBeenCalledWith({
        boardId: '00000000-0000-4000-8000-000000000003',
        title: 'Fix chat cwd',
        description: '## Context\n\nUnbound chat sessions need a safe cwd.',
      });
    });
    expect(mocks.openTicket).toHaveBeenCalledWith(
      '00000000-0000-4000-8000-000000000050',
    );
  });

  it('proposes knowledge from chat into the pending inbox', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.createKnowledge.mockResolvedValue({
      knowledge: { id: '00000000-0000-4000-8000-000000000060', status: 'pending' },
      message: {
        id: '00000000-0000-4000-8000-000000000061',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'system',
        body: 'Proposed knowledge',
        agentRunId: null,
        actionMetadata: {
          action: 'create_knowledge',
          knowledgeItemId: '00000000-0000-4000-8000-000000000060',
        },
        createdAt: '2026-09-08T00:03:00Z',
      },
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    fireEvent.click(screen.getByRole('button', { name: 'Propose knowledge' }));
    expect(screen.getByTestId('create-knowledge-dialog')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Knowledge title (optional)'), {
      target: { value: 'Resolve chat cwd under worktrees' },
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'Confirm propose knowledge' }),
    );

    await waitFor(() => {
      expect(mocks.createKnowledge).toHaveBeenCalledWith({
        title: 'Resolve chat cwd under worktrees',
        content: undefined,
        knowledgeType: 'coding_convention',
        scope: 'board',
        boardId: '00000000-0000-4000-8000-000000000003',
      });
    });
    expect(await screen.findByText('Knowledge inbox')).toBeInTheDocument();
  });


  it('submits on Enter with a non-empty body', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000031',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'human',
        body: 'Hello via Enter',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    fireEvent.change(input, { target: { value: 'Hello via Enter' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });

    await waitFor(() => {
      expect(mocks.postMessage).toHaveBeenCalledWith({
        body: 'Hello via Enter',
      });
    });
  });

  it('inserts a newline on Shift+Enter without submitting', async () => {
    mocks.sessions = [ACTIVE_SESSION];

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    fireEvent.change(input, { target: { value: 'line one' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter', shiftKey: true });

    expect(mocks.postMessage).not.toHaveBeenCalled();
    expect(input).toHaveValue('line one');
  });

  it('does nothing on Enter when the body is empty or whitespace', async () => {
    mocks.sessions = [ACTIVE_SESSION];

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
    expect(mocks.postMessage).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: '   ' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
    expect(mocks.postMessage).not.toHaveBeenCalled();
  });

  it('does not submit Enter while IME composition is active', async () => {
    mocks.sessions = [ACTIVE_SESSION];

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    fireEvent.change(input, { target: { value: 'こんにちは' } });
    fireEvent.keyDown(input, {
      key: 'Enter',
      code: 'Enter',
      isComposing: true,
    });
    expect(mocks.postMessage).not.toHaveBeenCalled();

    fireEvent.keyDown(input, {
      key: 'Enter',
      code: 'Enter',
      keyCode: 229,
    });
    expect(mocks.postMessage).not.toHaveBeenCalled();
  });

  it('does not submit Enter while a post is pending', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.postMessagePending = true;

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    // Pending disables the textarea; still assert key handler is a no-op.
    expect(input).toBeDisabled();
    fireEvent.change(input, { target: { value: 'Should not send' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });

    expect(mocks.postMessage).not.toHaveBeenCalled();
  });

  it('does not submit Enter while a turn is in flight', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.postMessage.mockResolvedValue({
      message: {
        id: '00000000-0000-4000-8000-000000000031',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'human',
        body: 'First',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: null,
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const input = await screen.findByLabelText('Message');
    fireEvent.change(input, { target: { value: 'First' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.postMessage).toHaveBeenCalledWith({ body: 'First' });
    });
    expect(await screen.findByTestId('chat-streaming-bubble')).toBeInTheDocument();
    expect(screen.queryByTestId('chat-live-turn')).toBeNull();

    mocks.postMessage.mockClear();
    const disabledInput = screen.getByLabelText('Message');
    expect(disabledInput).toBeDisabled();
    fireEvent.change(disabledInput, { target: { value: 'Second' } });
    fireEvent.keyDown(disabledInput, { key: 'Enter', code: 'Enter' });

    expect(mocks.postMessage).not.toHaveBeenCalled();
  });
});
