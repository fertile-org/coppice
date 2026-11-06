import { describe, expect, it } from 'vitest';
import { runUsageDetailSchema, ticketUsageSchema } from './usage';

// Shaped like the serializers in server/src/services/usage_service.rs.
const tokens = { input: 4, cacheRead: 182_000, cacheWrite: 24_000, output: 9_100, reasoning: null };
const totals = { status: 'reported', tokens, requestCount: 12, subagentCount: 1 };
const RUN = '00000000-0000-4000-8000-0000000000a1';
const AGENT = '00000000-0000-4000-8000-0000000000b1';

describe('usage schemas', () => {
  it('parses a server-shaped ticket usage payload', () => {
    const parsed = ticketUsageSchema.parse({
      runs: [{ runId: RUN, agentId: AGENT, jobType: 'work', tracked: true, usage: totals }],
      own: { key: 'ticket', runs: 1, runsWithUsage: 1, tokens },
      subtickets: { ticketCount: 2, runsWithUsage: 3, tokens },
      byAgent: [{ key: AGENT, runs: 1, runsWithUsage: 1, tokens }],
      byJobType: [{ key: 'work', runs: 1, runsWithUsage: 1, tokens }],
    });
    expect(parsed.own.runsWithUsage).toBe(1);
    expect(parsed.runs[0].usage?.requestCount).toBe(12);
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
