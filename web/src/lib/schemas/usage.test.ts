import { describe, expect, it } from 'vitest';
import { runUsageDetailSchema, ticketUsageSchema } from './usage';

// Shaped like the serializers in server/src/services/usage_service.rs.
const tokens = { input: 4, cacheRead: 182_000, cacheWrite: 24_000, output: 9_100, reasoning: null };
const totals = { status: 'reported', tokens, subagentTokens: null, requestCount: 12, subagentCount: 1 };
const group = { runsNotReported: 0, partialRuns: 0, subagentTokens: null, tokens };
const RUN = '00000000-0000-4000-8000-0000000000a1';
const AGENT = '00000000-0000-4000-8000-0000000000b1';

describe('usage schemas', () => {
  it('parses a server-shaped ticket usage payload', () => {
    const parsed = ticketUsageSchema.parse({
      runs: [
        { runId: RUN, agentId: AGENT, jobType: 'work', usage: totals },
        { runId: RUN, agentId: AGENT, jobType: 'work', usage: null },
      ],
      own: { ...group, key: 'ticket', runs: 1, runsWithUsage: 1 },
      subtickets: { ticketCount: 2, runsWithUsage: 3, partialRuns: 1, tokens, subagentTokens: 300 },
      byAgent: [{ ...group, key: AGENT, runs: 1, runsWithUsage: 1 }],
      byJobType: [{ ...group, key: 'work', runs: 1, runsWithUsage: 1 }],
    });
    expect(parsed.own.runsWithUsage).toBe(1);
    expect(parsed.runs[0].usage?.requestCount).toBe(12);
    expect(parsed.runs[1].usage).toBeNull();
    expect(parsed.subtickets?.subagentTokens).toBe(300);
    expect(
      runUsageDetailSchema.parse({
        totals: { ...totals, status: 'none', subagentTokens: null },
        requests: [],
        subagents: [],
      }).totals?.status,
    ).toBe('none');
  });

  it('parses a server-shaped run usage detail payload', () => {
    const parsed = runUsageDetailSchema.parse({
      totals,
      requests: [{ id: 'req-1', granularity: 'request', model: 'claude', tokens }],
      subagents: [
        {
          id: 'sub-1',
          agentType: 'Explore',
          description: null,
          model: null,
          status: 'completed',
          tokens,
          totalTokens: 215_104,
          toolUses: 3,
          durationMs: 1200,
        },
      ],
    });
    expect(parsed.subagents[0].totalTokens).toBe(215_104);
    expect(runUsageDetailSchema.parse({ totals: null, requests: [], subagents: [] }).totals).toBeNull();
  });
});
