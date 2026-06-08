import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TicketRunsTab } from './TicketRunsTab';
import type { AgentRun } from '../../lib/schemas/agentRun';

const failedRun: AgentRun = {
  id: '00000000-0000-0000-0000-000000000001',
  ticketId: '00000000-0000-0000-0000-000000000002',
  agentId: '00000000-0000-0000-0000-000000000003',
  jobType: 'work_on_ticket',
  status: 'failed',
  sandboxProfileId: 'permissive',
  worktreePath: null,
  branchName: null,
  errorMessage: 'ensure worktree: git command failed: fatal: path missing',
  startedAt: '2026-06-08T00:00:00.000Z',
  endedAt: '2026-06-08T00:00:01.000Z',
  createdAt: '2026-06-08T00:00:00.000Z',
};

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: [failedRun], isLoading: false, isError: false }),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [{ id: '00000000-0000-0000-0000-000000000003', name: 'Worker' }],
  }),
}));

function renderRuns() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <TicketRunsTab ticketId="00000000-0000-0000-0000-000000000002" />
    </QueryClientProvider>,
  );
}

describe('TicketRunsTab', () => {
  it('shows failed run error without extra click', () => {
    renderRuns();
    expect(screen.getByText(/path missing/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /show error/i })).toBeNull();
  });
});
