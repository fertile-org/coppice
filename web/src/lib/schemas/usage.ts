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

/**
 * `none`: Coppice reads this run's usage but nothing was reported.
 * `reported`: the CLI's own total. `partial`: the sum of what was streamed.
 */
export const usageStatusSchema = z.enum(['none', 'reported', 'partial']);

export type UsageStatus = z.infer<typeof usageStatusSchema>;

export const runUsageSchema = z.object({
  status: usageStatusSchema,
  tokens: tokenCountsSchema,
  /** Partial totals only: subagent tokens without a breakdown, on top of `tokens`. */
  subagentTokens: count,
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

/** Tracked runs only: runs Coppice could not read are left out. */
export const usageGroupSchema = z.object({
  key: z.string(),
  runs: z.number().int(),
  runsWithUsage: z.number().int(),
  /** Finished runs that reported nothing. */
  runsNotReported: z.number().int(),
  /** Finished runs whose total is only the sum of what they streamed. */
  partialRuns: z.number().int(),
  tokens: tokenCountsSchema,
  subagentTokens: count,
});

export type UsageGroup = z.infer<typeof usageGroupSchema>;

export const ticketUsageSchema = z.object({
  runs: z.array(
    z.object({
      runId: z.string().uuid(),
      agentId: z.string().uuid(),
      jobType: z.string(),
      /** `null` when Coppice could not read the run's usage. */
      usage: runUsageSchema.nullable(),
    }),
  ),
  own: usageGroupSchema,
  subtickets: z
    .object({
      ticketCount: z.number().int(),
      runsWithUsage: z.number().int(),
      partialRuns: z.number().int(),
      tokens: tokenCountsSchema,
      subagentTokens: count,
    })
    .nullable(),
  byAgent: z.array(usageGroupSchema),
  byJobType: z.array(usageGroupSchema),
});

export type TicketUsage = z.infer<typeof ticketUsageSchema>;
export type TicketRunUsage = TicketUsage['runs'][number];

export const usageGroupBySchema = z.enum(['model', 'agent', 'board', 'connector', 'jobType']);

export type UsageGroupBy = z.infer<typeof usageGroupBySchema>;

/** One group in Settings → Usage. `label` is null when the page names the key. */
export const usageSummaryRowSchema = z.object({
  key: z.string(),
  label: z.string().nullable(),
  connector: z.string().nullable(),
  tokens: tokenCountsSchema,
  /** Subagent totals without a breakdown, on top of `tokens`. */
  unsplit: z.number().int(),
  total: z.number().int(),
  runs: z.number().int(),
});

export type UsageSummaryRow = z.infer<typeof usageSummaryRowSchema>;

export const usageSummarySchema = z.object({
  totals: z.object({
    tokens: tokenCountsSchema,
    unsplit: z.number().int(),
    total: z.number().int(),
    runs: z.number().int(),
    activeDays: z.number().int(),
  }),
  rows: z.array(usageSummaryRowSchema),
  /** Days with tokens, oldest first, as `YYYY-MM-DD` in the viewer's time zone. */
  daily: z.array(z.object({ day: z.string(), byKey: z.record(z.number().int()) })),
  coverage: z.object({
    trackedRuns: z.number().int(),
    untrackedRuns: z.number().int(),
    untrackedByConnector: z.array(z.object({ connector: z.string(), runs: z.number().int() })),
  }),
  /** Top-level tickets that reached Done in the range, each with its whole tree. */
  perDoneTicket: z
    .object({ tickets: z.number().int(), median: z.number(), average: z.number() })
    .nullable(),
});

export type UsageSummary = z.infer<typeof usageSummarySchema>;
