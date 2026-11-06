import { z } from 'zod';

/** A null count means the connector did not report it. */
const count = z.number().int().nullable();

export const tokenCountsSchema = z.object({
  input: count,
  cacheRead: count,
  cacheWrite: count,
  output: count,
  reasoning: count,
});

export type TokenCounts = z.infer<typeof tokenCountsSchema>;

export const usageStatusSchema = z.enum(['reported', 'partial']);

export type UsageStatus = z.infer<typeof usageStatusSchema>;

export const runUsageSchema = z.object({
  status: usageStatusSchema,
  tokens: tokenCountsSchema,
  requestCount: z.number().int(),
  subagentCount: z.number().int(),
});

export type RunUsage = z.infer<typeof runUsageSchema>;

export const requestUsageSchema = z.object({
  id: z.string().nullable(),
  granularity: z.enum(['request', 'turn']),
  model: z.string().nullable(),
  tokens: tokenCountsSchema,
});

export type RequestUsage = z.infer<typeof requestUsageSchema>;

export const subagentUsageSchema = z.object({
  id: z.string(),
  agentType: z.string().nullable(),
  description: z.string().nullable(),
  model: z.string().nullable(),
  status: z.string().nullable(),
  tokens: tokenCountsSchema,
  totalTokens: count,
  toolUses: count,
  durationMs: count,
});

export type SubagentUsage = z.infer<typeof subagentUsageSchema>;

export const runUsageDetailSchema = z.object({
  totals: runUsageSchema.nullable(),
  requests: z.array(requestUsageSchema),
  subagents: z.array(subagentUsageSchema),
});

export type RunUsageDetail = z.infer<typeof runUsageDetailSchema>;

export const usageGroupSchema = z.object({
  key: z.string(),
  runs: z.number().int(),
  runsWithUsage: z.number().int(),
  tokens: tokenCountsSchema,
});

export type UsageGroup = z.infer<typeof usageGroupSchema>;

export const ticketUsageSchema = z.object({
  runs: z.array(
    z.object({
      runId: z.string().uuid(),
      agentId: z.string().uuid(),
      jobType: z.string(),
      tracked: z.boolean(),
      usage: runUsageSchema.nullable(),
    }),
  ),
  own: usageGroupSchema,
  subtickets: z
    .object({
      ticketCount: z.number().int(),
      runsWithUsage: z.number().int(),
      tokens: tokenCountsSchema,
    })
    .nullable(),
  byAgent: z.array(usageGroupSchema),
  byJobType: z.array(usageGroupSchema),
});

export type TicketUsage = z.infer<typeof ticketUsageSchema>;
export type TicketRunUsage = TicketUsage['runs'][number];
