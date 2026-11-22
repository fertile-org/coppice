import { z } from 'zod';
import { idSchema } from './id';

export const knowledgeStatusSchema = z.enum([
  'pending',
  'approved',
  'rejected',
  'stale',
]);

export const knowledgeScopeSchema = z.enum([
  'workspace',
  'board',
  'agent',
]);

export const knowledgeTypeSchema = z.enum([
  'coding_convention',
  'architecture_rule',
  'bug_pattern',
  'test_command',
  'review_feedback',
  'dependency_note',
  'api_contract',
  'workflow_rule',
  'human_preference',
  'operational_runbook',
  'security_rule',
  'performance_note',
]);

export const knowledgeSourceTypeSchema = z.enum([
  'ticket',
  'comment',
  'review',
  'human_note',
  'agent_summary',
  'workspace_signal',
  'observation_run',
  'chat_session',
]);

export const knowledgeConfidenceSchema = z.enum(['low', 'medium', 'high']);

export const knowledgeItemSchema = z.object({
  id: idSchema,
  version: z.number().int().positive(),
  status: knowledgeStatusSchema,
  revisionId: idSchema,
  revisionNumber: z.number().int().positive(),
  activeRevisionId: idSchema.nullable(),
  scope: knowledgeScopeSchema,
  boardId: idSchema.nullable(),
  boardName: z.string().nullable(),
  agentId: idSchema.nullable(),
  agentName: z.string().nullable(),
  knowledgeType: knowledgeTypeSchema,
  title: z.string(),
  content: z.string(),
  sourceType: knowledgeSourceTypeSchema,
  sourceId: idSchema.nullable(),
  sourceRunId: idSchema.nullable(),
  confidence: knowledgeConfidenceSchema,
  approvedBy: idSchema.nullable(),
  approvedAt: z.string().nullable(),
  approvalMode: z.string().nullable(),
  policyDecision: z.string().nullable(),
  policyReason: z.string().nullable(),
  rejectionReason: z.string().nullable(),
  expiresAt: z.string().nullable(),
  supersedesItemId: idSchema.nullable(),
  supersededBy: idSchema.nullable(),
  staleAt: z.string().nullable(),
  compactionBatchId: idSchema.nullable(),
  compactionAgentId: idSchema.nullable(),
  compactionAgentName: z.string().nullable(),
  sourceTicketIds: z.array(idSchema),
  usageCount: z.number().int().nonnegative(),
  openFlagCount: z.number().int().nonnegative(),
  lastUsedAt: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const knowledgePageSchema = z.object({
  items: z.array(knowledgeItemSchema),
  nextCursor: z.string().nullable(),
});

export const knowledgeUsageSchema = z.object({
  itemId: idSchema,
  revisionId: idSchema,
  rank: z.number().int().positive(),
  score: z.number(),
  tokenCount: z.number().int().nonnegative(),
  renderedContent: z.string(),
  title: z.string(),
  knowledgeType: knowledgeTypeSchema,
  scope: knowledgeScopeSchema,
  sourceType: knowledgeSourceTypeSchema,
  sourceId: idSchema.nullable(),
  includedAt: z.string(),
});

export const knowledgeUsageListSchema = z.object({
  items: z.array(knowledgeUsageSchema),
});

export const similarNeighborSchema = z.object({
  itemId: idSchema,
  revisionId: idSchema,
  title: z.string(),
  knowledgeType: knowledgeTypeSchema,
  scope: knowledgeScopeSchema,
  boardId: idSchema.nullable(),
  score: z.number(),
  status: z.string(),
});

export const similarListSchema = z.object({
  items: z.array(similarNeighborSchema),
});

export const knowledgeFreshnessStateSchema = z.enum([
  'current',
  'changed',
  'missing',
  'unknown',
  'uncited',
]);

export const knowledgeFreshnessSchema = z.object({
  state: knowledgeFreshnessStateSchema,
  citations: z.array(
    z.object({
      path: z.string(),
      repoName: z.string().nullable(),
      commitSha: z.string().nullable(),
    }),
  ),
});

export const knowledgeFlagListSchema = z.object({
  items: z.array(
    z.object({
      verdict: z.enum(['outdated', 'wrong']),
      reason: z.string(),
      agentName: z.string().nullable(),
      createdAt: z.string(),
    }),
  ),
});

export const compactionBatchSchema = z.object({
  id: idSchema,
  agentId: idSchema,
  agentName: z.string().nullable(),
  runId: idSchema.nullable(),
  status: z.enum(['queued', 'running', 'succeeded', 'failed']),
  trigger: z.enum(['scheduled', 'manual', 'retry']),
  ticketCount: z.number().int().nonnegative(),
  candidateCount: z.number().int().nonnegative().nullable(),
  summary: z.string().nullable(),
  errorMessage: z.string().nullable(),
  createdAt: z.string(),
  startedAt: z.string().nullable(),
  endedAt: z.string().nullable(),
});

export const compactionStateSchema = z.enum([
  'not_configured',
  'agent_disabled',
  'running',
  'failed',
  'idle',
]);

export const compactionStatusSchema = z.object({
  state: compactionStateSchema,
  configured: z.boolean(),
  agent: z
    .object({
      id: idSchema,
      name: z.string(),
      enabled: z.boolean(),
      connector: z.string(),
    })
    .nullable(),
  queuedCount: z.number().int().nonnegative(),
  blockedCount: z.number().int().nonnegative(),
  oldestQueuedAt: z.string().nullable(),
  oldestPendingAt: z.string().nullable(),
  activeBatch: compactionBatchSchema.nullable(),
  lastBatch: compactionBatchSchema.nullable(),
  nextScheduledAt: z.string().nullable(),
  intervalSecs: z.number().int().nonnegative(),
  batchMaxTickets: z.number().int().positive(),
});

export type KnowledgeStatus = z.infer<typeof knowledgeStatusSchema>;
export type KnowledgeScope = z.infer<typeof knowledgeScopeSchema>;
export type KnowledgeType = z.infer<typeof knowledgeTypeSchema>;
export type KnowledgeSourceType = z.infer<typeof knowledgeSourceTypeSchema>;
export type KnowledgeConfidence = z.infer<typeof knowledgeConfidenceSchema>;
export type KnowledgeItem = z.infer<typeof knowledgeItemSchema>;
export type KnowledgePage = z.infer<typeof knowledgePageSchema>;
export type KnowledgeUsage = z.infer<typeof knowledgeUsageSchema>;
export type SimilarNeighbor = z.infer<typeof similarNeighborSchema>;
export type KnowledgeFreshness = z.infer<typeof knowledgeFreshnessSchema>;
export type KnowledgeFlagList = z.infer<typeof knowledgeFlagListSchema>;
export type SimilarList = z.infer<typeof similarListSchema>;
export type CompactionBatch = z.infer<typeof compactionBatchSchema>;
export type CompactionState = z.infer<typeof compactionStateSchema>;
export type CompactionStatus = z.infer<typeof compactionStatusSchema>;
