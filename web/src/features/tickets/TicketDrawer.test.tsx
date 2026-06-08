import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TicketDrawer } from './TicketDrawer';
import type { Ticket } from '../board/useTickets';

const mockTicket: Ticket = {
  id: '00000000-0000-0000-0000-000000000001',
  projectId: '00000000-0000-0000-0000-000000000002',
  title: 'Test ticket',
  description: 'Ticket description',
  status: 'backlog',
  createdBy: 'user',
  createdAt: '2026-06-08T00:00:00.000Z',
  updatedAt: '2026-06-08T00:00:00.000Z',
  lastActivityAt: '2026-06-08T00:00:00.000Z',
};

vi.mock('./useTicket', () => ({
  useTicket: () => ({
    data: mockTicket,
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
  statusLabel: (status: string) => status,
  useUpdateTicket: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useAssignAgent: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useAgents: () => ({ data: [] }),
  useUpdateTicketStatus: () => ({ mutateAsync: vi.fn(), isPending: false }),
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
    <QueryClientProvider client={client}>
      <TicketDrawer
        ticketId="00000000-0000-0000-0000-000000000001"
        onClose={() => {}}
      />
    </QueryClientProvider>,
  );
}

describe('TicketDrawer', () => {
  it('renders Detail and Agent Runs tabs only', () => {
    renderDrawer();
    expect(screen.getByRole('tab', { name: 'Detail' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Agent Runs' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Comments' })).toBeNull();
  });

  it('drawer panel uses 90% width class', () => {
    renderDrawer();
    const dialog = screen.getByRole('dialog', { name: 'Ticket detail' });
    expect(dialog.className).toMatch(/w-\[90%\]/);
  });
});
