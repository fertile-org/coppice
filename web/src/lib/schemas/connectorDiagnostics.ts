import { z } from 'zod';

export const probeStatusSchema = z
  .enum(['ok', 'failed', 'timed_out', 'not_run'])
  .catch('not_run');

export type ProbeStatus = z.infer<typeof probeStatusSchema>;

export const authStatusSchema = z
  .enum(['detected', 'not_found', 'verified_by_probe'])
  .catch('not_found');

export type AuthStatus = z.infer<typeof authStatusSchema>;

export const checkStatusSchema = z
  .enum(['queued', 'running', 'passed', 'failed'])
  .catch('failed');

export type CheckStatus = z.infer<typeof checkStatusSchema>;

export const lastRunSchema = z.object({
  runId: z.string(),
  ticketId: z.string().nullish(),
  status: z.string(),
  finishedAt: z.string(),
  ticketGet: z.boolean(),
  resultSubmit: z.boolean(),
});

export type LastRun = z.infer<typeof lastRunSchema>;

export const checkSummarySchema = z.object({
  id: z.string(),
  status: checkStatusSchema,
  failure: z.string().nullish(),
  createdAt: z.string(),
});

export type CheckSummary = z.infer<typeof checkSummarySchema>;

export const connectorStatusSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  enabled: z.boolean(),
  cli: z.object({
    found: z.boolean(),
    path: z.string().nullish(),
    probe: z.object({
      status: probeStatusSchema,
      detail: z.string().nullish(),
    }),
  }),
  auth: z.object({
    status: authStatusSchema,
    envSet: z.array(z.string()).default([]),
    pathsFound: z.array(z.string()).default([]),
  }),
  authHint: z.string(),
  docsUrl: z.string(),
  lastRun: lastRunSchema.nullish(),
  lastCheck: checkSummarySchema.nullish(),
  probedAt: z.string().nullish(),
});

export type ConnectorStatus = z.infer<typeof connectorStatusSchema>;

export const connectorStatusListSchema = z.array(connectorStatusSchema);

export const connectorCheckSchema = z.object({
  id: z.string(),
  connector: z.string(),
  agentId: z.string(),
  status: checkStatusSchema,
  failure: z.string().nullish(),
  runId: z.string().nullish(),
  createdAt: z.string(),
  finishedAt: z.string().nullish(),
  toolCalls: z
    .array(z.object({ tool: z.string(), status: z.string() }))
    .default([]),
});

export type ConnectorCheck = z.infer<typeof connectorCheckSchema>;

export const startConnectorTestSchema = z.object({
  checkId: z.string(),
  runId: z.string(),
});

export type StartConnectorTest = z.infer<typeof startConnectorTestSchema>;
