import '@testing-library/jest-dom/vitest';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import type { AgentRun } from '../../lib/schemas/agentRun';
import { NOT_COVERED_BY_PLAN } from '../runs/reportedModel';
import { TicketRunsTab } from './TicketRunsTab';

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: 'member' }, loading: false }),
}));

const billedRun: AgentRun = {
  id: '00000000-0000-0000-0000-000000000001',
  ticketId: '00000000-0000-0000-0000-000000000002',
  agentId: '00000000-0000-0000-0000-000000000003',
  jobType: 'work_on_ticket',
  status: 'succeeded',
  sandboxProfileId: 'permissive',
  worktreePath: null,
  branchName: null,
  errorMessage: null,
  startedAt: '2026-06-08T00:00:00.000Z',
  endedAt: '2026-06-08T00:00:01.000Z',
  createdAt: '2026-06-08T00:00:00.000Z',
  reportedModel: 'composer-2.5',
  billedOutsidePlan: true,
};

vi.mock('./useAgentRuns', () => ({
  useAgentRuns: () => ({ data: [billedRun], isLoading: false, isError: false }),
}));

vi.mock('../runs/useRunToolCalls', () => ({
  useRunToolCalls: () => ({
    data: { items: [], skillsUsed: [] },
    isLoading: false,
    isError: false,
  }),
}));

vi.mock('../knowledge/useKnowledge', () => ({
  useKnowledgeUsed: () => ({ data: [], isLoading: false, isError: false }),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [{ id: '00000000-0000-0000-0000-000000000003', name: 'Worker' }],
  }),
}));

describe('TicketRunsTab model line', () => {
  it('shows the reported model and one quiet outside-plan line', () => {
    const client = new QueryClient();
    render(
      <MemoryRouter>
        <QueryClientProvider client={client}>
          <TicketRunsTab ticketId="00000000-0000-0000-0000-000000000002" />
        </QueryClientProvider>
      </MemoryRouter>,
    );
    expect(screen.getByText('composer-2.5')).toBeVisible();
    expect(screen.getByText(NOT_COVERED_BY_PLAN)).toBeVisible();
  });
});
