import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
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

const { useRunToolCalls, useKnowledgeUsed } = vi.hoisted(() => ({
  useRunToolCalls: vi.fn(() => ({
    data: { items: [], skillsUsed: [] },
    isLoading: false,
    isError: false,
  })),
  useKnowledgeUsed: vi.fn(() => ({ data: [], isLoading: false, isError: false })),
}));

vi.mock('../runs/useRunToolCalls', () => ({ useRunToolCalls }));
vi.mock('../knowledge/useKnowledge', () => ({ useKnowledgeUsed }));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [{ id: '00000000-0000-0000-0000-000000000003', name: 'Worker' }],
  }),
}));

function renderRuns() {
  const client = new QueryClient();
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <TicketRunsTab ticketId="00000000-0000-0000-0000-000000000002" />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe('TicketRunsTab', () => {
  it('shows failed run error without extra click', () => {
    renderRuns();
    expect(screen.getByText(/path missing/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /show error/i })).toBeNull();
  });

  it('details have tabs', () => {
    renderRuns();
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(useRunToolCalls).not.toHaveBeenCalled();
    expect(useKnowledgeUsed).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Show run details' }));

    const tools = screen.getByRole('tab', { name: 'Tools & Skills' });
    const knowledge = screen.getByRole('tab', { name: 'Knowledge Used' });
    expect(tools).toHaveAttribute('aria-selected', 'true');
    expect(knowledge).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByRole('tabpanel', { name: 'Tools & Skills' })).toBeVisible();
    expect(screen.getByText('No tool calls recorded.')).toBeVisible();
    expect(useRunToolCalls).toHaveBeenLastCalledWith(failedRun.id, true);
    expect(useKnowledgeUsed).toHaveBeenLastCalledWith(failedRun.id, false);

    fireEvent.click(knowledge);

    expect(knowledge).toHaveAttribute('aria-selected', 'true');
    expect(tools).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByRole('tabpanel', { name: 'Knowledge Used' })).toBeVisible();
    expect(screen.getByText('This run did not include stored knowledge.')).toBeVisible();
    expect(screen.queryByText('No tool calls recorded.')).toBeNull();
    expect(useKnowledgeUsed).toHaveBeenLastCalledWith(failedRun.id, true);
    expect(useRunToolCalls).toHaveBeenLastCalledWith(failedRun.id, false);
  });
});
