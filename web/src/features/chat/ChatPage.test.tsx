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
  acceptProposal: vi.fn(),
  rejectProposal: vi.fn(),
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
  useAcceptChatProposal: () => ({
    mutateAsync: mocks.acceptProposal,
    isPending: false,
  }),
  useRejectChatProposal: () => ({
    mutateAsync: mocks.rejectProposal,
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
    mocks.acceptProposal.mockReset();
    mocks.rejectProposal.mockReset();
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

  it('puts the most recently chatted agent first and the rest by name', () => {
    mocks.sessions = [
      {
        ...ACTIVE_SESSION,
        id: '00000000-0000-4000-8000-000000000041',
        agentId: '00000000-0000-4000-8000-000000000012',
        updatedAt: '2026-09-08T00:01:00Z',
      },
      {
        ...ACTIVE_SESSION,
        id: '00000000-0000-4000-8000-000000000042',
        agentId: '00000000-0000-4000-8000-000000000013',
        updatedAt: '2026-09-08T00:05:00Z',
      },
    ];
    renderChat('/chat');

    const text = screen.getByTestId('chat-session-list').textContent ?? '';
    const order = ['Codex Agent', 'Kilo Agent', 'Backend Engineer', 'Cursor Agent', 'OpenCode Agent'].map(
      (name) => text.indexOf(name),
    );
    expect(order.every((index) => index >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
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

  it('opens the newest chat when the page opens', async () => {
    mocks.sessions = [
      { ...ACTIVE_SESSION, title: 'Older', updatedAt: '2026-09-08T00:01:00Z' },
      {
        ...ACTIVE_SESSION,
        id: '00000000-0000-4000-8000-000000000099',
        title: 'Newest',
        updatedAt: '2026-09-08T00:05:00Z',
      },
    ];
    renderChat('/chat');

    expect(await screen.findByTestId('chat-session-pane')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Newest' })).toHaveAttribute('aria-current', 'page');
  });

  it('opens a new chat with the first agent when there are no chats', async () => {
    renderChat('/chat');

    expect(
      await screen.findByText('Say hi to Backend Engineer. They can see all your projects.'),
    ).toBeInTheDocument();
  });

  it('puts New chat in the open chat header, not on every agent row', async () => {
    mocks.sessions = [{ ...ACTIVE_SESSION, title: 'Fix the login form' }];
    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const header = await screen.findByTestId('chat-session-header');
    const button = within(header).getByRole('button', {
      name: 'New chat with Backend Engineer',
    });
    expect(button).toHaveAttribute('title', 'New chat with Backend Engineer');
    const list = screen.getByTestId('chat-session-list');
    expect(within(list).queryByRole('button', { name: /New chat/ })).toBeNull();

    fireEvent.click(button);
    expect(
      await screen.findByText('Say hi to Backend Engineer. They can see all your projects.'),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId('chat-session-header')).queryByRole('button', {
        name: /New chat/,
      }),
    ).toBeNull();
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

  it.each([
    ['OpenCode', '00000000-0000-4000-8000-000000000014'],
    ['Codex', '00000000-0000-4000-8000-000000000013'],
  ])('hides the isolation banner for a %s session', (_name, agentId) => {
    mocks.sessions = [
      {
        ...ACTIVE_SESSION,
        agentId,
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
    expect(screen.queryByRole('button', { name: 'Create ticket' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Propose knowledge' })).not.toBeInTheDocument();
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

  it('shows a pending ticket proposal under the agent reply and creates it on click', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.messages = [
      {
        id: '00000000-0000-4000-8000-000000000070',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'agent',
        body: 'Draft ready for you to confirm.',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: {
          proposals: [
            {
              id: '00000000-0000-4000-8000-000000000071',
              type: 'create_ticket',
              status: 'pending',
              title: 'Harden chat cwd resolution',
              description: '## Context\n\nUnbound chat sessions need a safe cwd.',
            },
          ],
        },
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
    ];
    mocks.acceptProposal.mockResolvedValue({
      message: mocks.messages[0],
      ticket: {
        id: '00000000-0000-4000-8000-000000000050',
        boardId: ACTIVE_SESSION.boardId,
        title: 'Harden chat cwd resolution',
        status: 'backlog',
      },
    });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    const card = screen.getByTestId('chat-proposal-card');
    expect(card).toHaveAttribute('data-status', 'pending');
    expect(within(card).getByText('Harden chat cwd resolution')).toBeInTheDocument();
    expect(within(card).queryByRole('combobox', { name: 'Board' })).not.toBeInTheDocument();

    fireEvent.click(within(card).getByRole('button', { name: 'Create ticket' }));

    await waitFor(() => {
      expect(mocks.acceptProposal).toHaveBeenCalledWith({
        messageId: '00000000-0000-4000-8000-000000000070',
        proposalId: '00000000-0000-4000-8000-000000000071',
        title: 'Harden chat cwd resolution',
        description: '## Context\n\nUnbound chat sessions need a safe cwd.',
        boardId: ACTIVE_SESSION.boardId,
      });
    });
    expect(mocks.openTicket).not.toHaveBeenCalled();
  });

  it('rejects a proposal from the chat card without creating anything', async () => {
    mocks.sessions = [ACTIVE_SESSION];
    mocks.messages = [
      {
        id: '00000000-0000-4000-8000-000000000080',
        sessionId: ACTIVE_SESSION.id,
        seq: 2,
        role: 'agent',
        body: 'Want a ticket?',
        agentRunId: '00000000-0000-4000-8000-000000000040',
        actionMetadata: {
          proposals: [
            {
              id: '00000000-0000-4000-8000-000000000081',
              type: 'create_ticket',
              status: 'pending',
              title: 'Not wanted',
            },
          ],
        },
        attachmentIds: [],
        attachments: [],
        createdAt: '2026-09-08T00:02:00Z',
      },
    ];
    mocks.rejectProposal.mockResolvedValue({ ...mocks.messages[0] });

    renderChat(`/chat/${ACTIVE_SESSION.id}`);

    fireEvent.click(
      within(screen.getByTestId('chat-proposal-card')).getByRole('button', { name: 'Reject' }),
    );

    await waitFor(() => {
      expect(mocks.rejectProposal).toHaveBeenCalledWith({
        messageId: '00000000-0000-4000-8000-000000000080',
        proposalId: '00000000-0000-4000-8000-000000000081',
      });
    });
    expect(mocks.acceptProposal).not.toHaveBeenCalled();
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
