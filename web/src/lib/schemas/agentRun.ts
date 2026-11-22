import { z } from 'zod';
import { connectorConsoleSchema } from './connector';
import { idSchema } from './id';

export const runStatusSchema = z.enum([
  'queued',
  'running',
  'succeeded',
  'failed',
  'blocked',
  'cancelled',
]);

export type RunStatus = z.infer<typeof runStatusSchema>;

export const agentRunSchema = z.object({
  id: idSchema,
  ticketId: idSchema,
  agentId: idSchema,
  jobType: z.string(),
  status: runStatusSchema,
  sandboxProfileId: z.string(),
  worktreePath: z.string().nullable(),
  branchName: z.string().nullable(),
  startedAt: z.string().nullable(),
  endedAt: z.string().nullable(),
  createdAt: z.string(),
  errorMessage: z.string().nullable(),
  sessionId: z.string().nullable().optional(),
  connector: z.string().optional(),
  /** Exact CLI model. Null or absent means the CLI did not report one. */
  reportedModel: z.string().nullable().optional(),
  billedOutsidePlan: z.boolean().optional(),
  /** The run's own console format; null means the connector's default. */
  consoleKind: connectorConsoleSchema.nullable().optional().catch(null),
});

export type AgentRun = z.infer<typeof agentRunSchema>;
