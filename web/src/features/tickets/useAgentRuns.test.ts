import { describe, expect, it } from 'vitest';
import { QueryClient } from '@tanstack/react-query';
import { agentRunsQueryKey } from './useAgentRuns';
import type { AgentRun } from '../../lib/schemas/agentRun';

function upsertRun(
  queryClient: QueryClient,
  ticketId: string,
  run: AgentRun,
) {
  queryClient.setQueryData<AgentRun[]>(agentRunsQueryKey(ticketId), (old) => {
    const prev = old ?? [];
    const index = prev.findIndex((item) => item.id === run.id);
    if (index === -1) {
      return [run, ...prev];
    }
    const next = [...prev];
    next[index] = run;
    return next;
  });
}

const baseRun: AgentRun = {
  id: '11111111-1111-1111-1111-111111111111',
  ticketId: '22222222-2222-2222-2222-222222222222',
  agentId: '33333333-3333-3333-3333-333333333333',
  jobType: 'implement',
  status: 'queued',
  sandboxProfileId: 'default',
  worktreePath: null,
  branchName: null,
  startedAt: null,
  endedAt: null,
  createdAt: '2026-01-01T00:00:00Z',
  errorMessage: null,
  connector: 'mock',
};

describe('agent runs cache upsert', () => {
  it('prepends a new run so the live console can connect immediately', () => {
    const queryClient = new QueryClient();
    const ticketId = baseRun.ticketId;
    const previousRun: AgentRun = {
      ...baseRun,
      id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
      status: 'succeeded',
    };

    queryClient.setQueryData(agentRunsQueryKey(ticketId), [previousRun]);
    upsertRun(queryClient, ticketId, baseRun);

    expect(queryClient.getQueryData<AgentRun[]>(agentRunsQueryKey(ticketId))).toEqual([
      baseRun,
      previousRun,
    ]);
  });
});
