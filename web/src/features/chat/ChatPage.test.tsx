import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatPage } from './ChatPage';

const mocks = vi.hoisted(() => ({
  createSession: vi.fn(),
  postMessage: vi.fn(),
  sessions: [] as Array<{
    id: string;
    projectId: string | null;
    ownerUserId: string;
    agentId: string;
    repoId: string | null;
    status: 'active' | 'archived' | 'cutoff';
    createdAt: string;
    updatedAt: string;
  }>,
  messages: [] as Array<{
    id: string;
    sessionId: string;
    seq: number;
    role: 'human' | 'agent' | 'system';
    body: string;
    agentRunId: string | null;
    actionMetadata: unknown;
    createdAt: string;
  }>,
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [
      {
        id: '00000000-0000-4000-8000-000000000010',
        name: 'Backend Engineer',
        enabled: true,
      },
      {
        id: '00000000-0000-4000-8000-000000000011',
        name: 'Disabled Agent',
        enabled: false,
      },
    ],
  }),
}));

vi.mock('../projects/useProjects', () => ({
  useProjects: () => ({
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

vi.mock('./useChat', () => ({
  useChatSessions: () => ({
    data: mocks.sessions,
    isLoading: false,
  }),
  useChatSession: (sessionId: string | null) => ({
    data: mocks.sessions.find((session) => session.id === sessionId),
  }),
  useChatMessages: () => ({
    data: mocks.messages,
    refetch: vi.fn(),
  }),
  useCreateChatSession: () => ({
    mutateAsync: mocks.createSession,
    isPending: false,
  }),
  usePostChatMessage: () => ({
    mutateAsync: mocks.postMessage,
    isPending: false,
  }),
}));

vi.mock('./ChatLiveTurn', () => ({
  ChatLiveTurn: ({ runId }: { runId: string }) => (
    <div data-testid="chat-live-turn">live:{runId}</div>
  ),
}));

function renderChat(path = '/chat') {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/chat" element={<ChatPage />} />
        <Route path="/chat/:sessionId" element={<ChatPage />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe('ChatPage', () => {
  beforeEach(() => {
    mocks.createSession.mockReset();
    mocks.postMessage.mockReset();
    mocks.sessions = [];
    mocks.messages = [];
  });

  it('shows new-chat controls for agent and optional project', () => {
    renderChat('/chat');

    expect(screen.getByTestId('new-chat-form')).toBeInTheDocument();
    const agent = screen.getByLabelText('Agent');
    expect(agent).toBeInTheDocument();
    expect(screen.getByLabelText('Project (optional)')).toBeInTheDocument();
    expect(agent).toHaveTextContent('Backend Engineer');
    expect(agent).not.toHaveTextContent('Disabled Agent');
  });

  it('creates a session then opens the transcript composer', async () => {
    mocks.createSession.mockResolvedValue({
      id: '00000000-0000-4000-8000-000000000001',
      projectId: '00000000-0000-4000-8000-000000000003',
      ownerUserId: '00000000-0000-4000-8000-000000000002',
      agentId: '00000000-0000-4000-8000-000000000010',
      repoId: null,
      status: 'active',
      createdAt: '2026-09-08T00:00:00Z',
      updatedAt: '2026-09-08T00:00:00Z',
    });

    renderChat('/chat');

    fireEvent.change(screen.getByLabelText('Agent'), {
      target: { value: '00000000-0000-4000-8000-000000000010' },
    });
    fireEvent.change(screen.getByLabelText('Project (optional)'), {
      target: { value: '00000000-0000-4000-8000-000000000003' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Start chat' }));

    await waitFor(() => {
      expect(mocks.createSession).toHaveBeenCalledWith({
        agentId: '00000000-0000-4000-8000-000000000010',
        projectId: '00000000-0000-4000-8000-000000000003',
      });
    });
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

    mocks.sessions = [
      {
        id: '00000000-0000-4000-8000-000000000001',
        projectId: null,
        ownerUserId: '00000000-0000-4000-8000-000000000002',
        agentId: '00000000-0000-4000-8000-000000000010',
        repoId: null,
        status: 'active',
        createdAt: '2026-09-08T00:00:00Z',
        updatedAt: '2026-09-08T00:00:00Z',
      },
    ];
    mocks.messages = [
      {
        id: '00000000-0000-4000-8000-000000000030',
        sessionId: '00000000-0000-4000-8000-000000000001',
        seq: 1,
        role: 'human',
        body: 'Prior message',
        agentRunId: null,
        actionMetadata: null,
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
        createdAt: '2026-09-08T00:02:00Z',
      },
      runId: '00000000-0000-4000-8000-000000000040',
    });

    renderChat('/chat/00000000-0000-4000-8000-000000000001');

    expect(await screen.findByText('Prior message')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Message'), {
      target: { value: 'What is cwd?' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.postMessage).toHaveBeenCalledWith('What is cwd?');
    });
    expect(await screen.findByTestId('chat-live-turn')).toHaveTextContent(
      'live:00000000-0000-4000-8000-000000000040',
    );
  });
});
