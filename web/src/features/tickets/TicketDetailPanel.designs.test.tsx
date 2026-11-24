import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { KeyboardProvider } from '../keyboard/KeyboardProvider';
import type { Ticket } from '../board/useTickets';
import { makeDesignSet, makeOption, makePick } from '../designs/designFixtures';
import { TicketDetailPanel } from './TicketDetailPanel';

const designs = vi.hoisted(() => ({
  data: [] as ReturnType<typeof makeDesignSet>[],
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({ data: [] }),
}));

vi.mock('./useTicket', () => ({
  useTicketChildren: () => ({ data: [] }),
  useUpdateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useUploadAttachment: () => ({ mutateAsync: vi.fn(), isPending: false }),
  COMMENTS_PAGE_SIZE: 200,
  fetchOlderComments: vi.fn(),
  useComments: () => ({ data: [], isLoading: false, isError: false }),
  useCreateComment: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTicketGitInfo: () => ({ data: undefined, isLoading: false }),
  ticketQueryKey: (ticketId: string) => ['ticket', ticketId],
  commentsQueryKey: (ticketId: string) => ['comments', ticketId],
}));

vi.mock('../designs/useDesigns', () => ({
  useDesigns: () => ({ data: designs.data }),
}));

vi.mock('./useAgentRuns', () => ({
  isActiveRunStatus: () => false,
  useAgentRuns: () => ({ data: [] }),
}));

const ticket: Ticket = {
  id: '00000000-0000-0000-0000-000000000001',
  boardId: '00000000-0000-0000-0000-000000000002',
  title: 'From chat ticket',
  description: 'Description body',
  status: 'backlog',
  createdBy: 'user',
  createdAt: '2026-06-08T00:00:00.000Z',
  updatedAt: '2026-06-08T00:00:00.000Z',
  lastActivityAt: '2026-06-08T00:00:00.000Z',
  attachmentIds: [],
  attachments: [],
};

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <KeyboardProvider>
          <ToastProvider>
            <TicketDetailPanel ticket={ticket} />
          </ToastProvider>
        </KeyboardProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('TicketDetailPanel chat designs', () => {
  it('shows a Design section for live chat-copied sets with a From chat tag', () => {
    const option = makeOption('a', 'Question card', 'A');
    designs.data = [
      makeDesignSet({
        title: 'Chat question UI',
        fromChat: true,
        fromPlanRun: false,
        chosen: makePick(option),
        recommended: makePick(option),
        selected: makePick(option),
        options: [option, makeOption('b', 'Quick replies', 'B')],
        choice: { mode: 'save_only', planOutOfDate: false },
      }),
      makeDesignSet({
        id: '33333333-3333-4333-8333-333333333333',
        title: 'Plan-only set',
        fromChat: false,
        fromPlanRun: true,
      }),
    ];
    renderPanel();
    const section = screen.getByTestId('ticket-chat-designs');
    expect(section).toHaveTextContent('Design');
    expect(section).toHaveTextContent('Chat question UI');
    expect(section).toHaveTextContent('From chat');
    expect(section).not.toHaveTextContent('Plan-only set');
  });

  it('hides the Design section when there are no chat-copied sets', () => {
    designs.data = [
      makeDesignSet({
        title: 'Plan set',
        fromChat: false,
        fromPlanRun: true,
      }),
    ];
    renderPanel();
    expect(screen.queryByTestId('ticket-chat-designs')).not.toBeInTheDocument();
  });
});
