import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { TicketDrawer } from './TicketDrawer';
import type { Ticket } from '../board/useTickets';

const ticketState: { ticket: Ticket } = {
  ticket: {
    id: '00000000-0000-0000-0000-000000000001',
    projectId: '00000000-0000-0000-0000-000000000002',
    title: 'Test ticket',
    description: 'Ticket description',
    status: 'backlog',
    createdBy: 'user',
    createdAt: '2026-06-08T00:00:00.000Z',
    updatedAt: '2026-06-08T00:00:00.000Z',
    lastActivityAt: '2026-06-08T00:00:00.000Z',
  },
};

vi.mock('./useTicket', () => ({
  useTicket: () => ({
    data: ticketState.ticket,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  statusLabel: (status: string) => status,
  useUpdateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useAssignAgent: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useAgents: () => ({ data: [] }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useFinalApprove: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTicketChildren: () => ({ data: [] }),
  useApproveSplits: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDismissSplits: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTicketGitInfo: () => ({ data: undefined, isLoading: false }),
  useMergeTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRemoveWorktree: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: [], isLoading: false, isError: false }),
  useRunAgent: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useStopRun: () => ({ mutateAsync: vi.fn(), isPending: false }),
  isActiveRunStatus: () => false,
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: [] }),
}));

vi.mock('./TicketCommentsTab', () => ({
  TicketCommentsTab: () => <div>Comments</div>,
}));

function renderDrawer() {
  const client = new QueryClient();
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <ToastProvider>
          <TicketDrawer
            ticketId="00000000-0000-0000-0000-000000000001"
            onClose={() => {}}
          />
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('TicketDrawer', () => {
  it('renders Detail, Live Console, and Agent Runs tabs', () => {
    renderDrawer();
    expect(screen.getByRole('tab', { name: 'Detail' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Live Console' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Agent Runs' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Comments' })).toBeNull();
  });

  it('drawer panel uses 90% width class', () => {
    renderDrawer();
    const dialog = screen.getByRole('dialog', { name: 'Ticket detail' });
    expect(dialog.className).toMatch(/w-\[90%\]/);
  });

  it('shows Final Approve only when status is wait_for_final_review', () => {
    ticketState.ticket = { ...ticketState.ticket, status: 'backlog' };
    const { unmount } = renderDrawer();
    expect(screen.queryByRole('button', { name: 'Final Approve' })).toBeNull();
    unmount();

    ticketState.ticket = {
      ...ticketState.ticket,
      status: 'wait_for_final_review',
    };
    renderDrawer();
    expect(
      screen.getByRole('button', { name: 'Final Approve' }),
    ).toBeInTheDocument();
  });
});
