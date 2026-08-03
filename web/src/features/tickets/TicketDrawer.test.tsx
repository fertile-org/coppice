import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { ToastProvider } from '../../components/ToastProvider';
import { TicketDrawer } from './TicketDrawer';
import type { Ticket } from '../board/useTickets';
import type { TicketParentSummary } from '../board/ticketHierarchy';
import type { AgentRun } from '../../lib/schemas/agentRun';

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

const runsState: { runs: AgentRun[] } = {
  runs: [],
};

const childrenState: { children: Ticket[] } = {
  children: [],
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
  useTicketChildren: () => ({ data: childrenState.children }),
  useApproveSplits: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useDismissSplits: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useTicketGitInfo: () => ({ data: undefined, isLoading: false }),
  useMergeTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRemoveWorktree: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: runsState.runs, isLoading: false, isError: false }),
  useRunAgent: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useStopRun: () => ({ mutateAsync: vi.fn(), isPending: false }),
  isActiveRunStatus: (status: string) => status === 'running' || status === 'queued',
  shouldPollRunForReconciliation: (run: AgentRun) =>
    run.status === 'running' || run.status === 'queued' || run.endedAt == null,
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: [] }),
}));

vi.mock('../runs/LiveConsole', () => ({
  LiveConsole: () => <div>Live console</div>,
}));

vi.mock('../runs/LiveSession', () => ({
  LiveSession: () => <div>Live session</div>,
}));

vi.mock('../runs/ClaudeLiveConsole', () => ({
  ClaudeLiveConsole: () => <div>Claude live console</div>,
}));

vi.mock('./TicketCommentsTab', () => ({
  TicketCommentsTab: () => <div>Comments</div>,
}));

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location-search">{location.search}</output>;
}

function renderDrawer(parentTicket: TicketParentSummary | null = null) {
  const client = new QueryClient();
  return render(
    <MemoryRouter initialEntries={[`/?ticket=${ticketState.ticket.id}`]}>
      <QueryClientProvider client={client}>
        <ToastProvider>
          <TicketDrawer
            ticketId="00000000-0000-0000-0000-000000000001"
            parentTicket={parentTicket}
            onClose={() => {}}
          />
          <LocationProbe />
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('TicketDrawer', () => {
  beforeEach(() => {
    runsState.runs = [];
    childrenState.children = [];
    ticketState.ticket = {
      ...ticketState.ticket,
      status: 'backlog',
      parentTicketId: undefined,
    };
  });

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

  it('routes kilo-code runs through the structured live console', () => {
    runsState.runs = [
      {
        id: '00000000-0000-0000-0000-000000000010',
        ticketId: ticketState.ticket.id,
        agentId: '00000000-0000-0000-0000-000000000011',
        jobType: 'agent_run',
        status: 'running',
        sandboxProfileId: 'default',
        errorMessage: null,
        worktreePath: null,
        branchName: null,
        startedAt: '2026-06-08T00:00:00.000Z',
        endedAt: null,
        createdAt: '2026-06-08T00:00:00.000Z',
        connector: 'kilo-code',
      },
    ];

    renderDrawer();
    fireEvent.click(screen.getByRole('tab', { name: 'Live Console' }));

    expect(screen.getByText('Claude live console')).toBeInTheDocument();
  });

  it('opens a child ticket parent in the same drawer route', () => {
    const parent = {
      id: '00000000-0000-0000-0000-000000000020',
      title: 'Parent roadmap ticket',
    };
    ticketState.ticket = {
      ...ticketState.ticket,
      parentTicketId: parent.id,
    };

    renderDrawer(parent);

    expect(screen.getByText('Parent ticket')).toBeVisible();
    const parentButton = screen.getByRole('button', {
      name: `Open parent ticket: ${parent.title}`,
    });
    expect(parentButton).toHaveTextContent(parent.title);

    fireEvent.click(parentButton);

    expect(screen.getByTestId('location-search')).toHaveTextContent(
      `?ticket=${parent.id}`,
    );
  });

  it('preserves navigation from a parent to its child tickets', () => {
    const child = {
      ...ticketState.ticket,
      id: '00000000-0000-0000-0000-000000000021',
      title: 'Existing child ticket',
      parentTicketId: ticketState.ticket.id,
    };
    childrenState.children = [child];

    renderDrawer();

    const childButton = screen.getByText(child.title).closest('button');
    expect(childButton).not.toBeNull();
    fireEvent.click(childButton!);

    expect(screen.getByTestId('location-search')).toHaveTextContent(
      `?ticket=${child.id}`,
    );
  });
});
