import { z } from 'zod';

export const knowledgeStatusSchema = z.enum([
  'pending',
  'approved',
  'rejected',
  'stale',
]);

export const knowledgeScopeSchema = z.enum([
  'workspace',
  'project',
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
]);

export const knowledgeConfidenceSchema = z.enum(['low', 'medium', 'high']);

export const knowledgeItemSchema = z.object({
  id: z.string().uuid(),
  version: z.number().int().positive(),
  status: knowledgeStatusSchema,
  revisionId: z.string().uuid(),
  revisionNumber: z.number().int().positive(),
  activeRevisionId: z.string().uuid().nullable(),
  scope: knowledgeScopeSchema,
  projectId: z.string().uuid().nullable(),
  projectName: z.string().nullable(),
  agentId: z.string().uuid().nullable(),
  agentName: z.string().nullable(),
  knowledgeType: knowledgeTypeSchema,
  title: z.string(),
  content: z.string(),
  sourceType: knowledgeSourceTypeSchema,
  sourceId: z.string().uuid().nullable(),
  sourceRunId: z.string().uuid().nullable(),
  confidence: knowledgeConfidenceSchema,
  approvedBy: z.string().uuid().nullable(),
  approvedAt: z.string().nullable(),
  approvalMode: z.string().nullable(),
  policyDecision: z.string().nullable(),
  policyReason: z.string().nullable(),
  rejectionReason: z.string().nullable(),
  expiresAt: z.string().nullable(),
  supersedesItemId: z.string().uuid().nullable(),
  supersededBy: z.string().uuid().nullable(),
  staleAt: z.string().nullable(),
  embeddingStatus: z.string(),
  embeddingError: z.string().nullable(),
  usageCount: z.number().int().nonnegative(),
  lastUsedAt: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const knowledgePageSchema = z.object({
  items: z.array(knowledgeItemSchema),
  nextCursor: z.string().nullable(),
});

export const knowledgeUsageSchema = z.object({
  itemId: z.string().uuid(),
  revisionId: z.string().uuid(),
  rank: z.number().int().positive(),
  similarity: z.number(),
  tokenCount: z.number().int().nonnegative(),
  renderedContent: z.string(),
  title: z.string(),
  knowledgeType: knowledgeTypeSchema,
  scope: knowledgeScopeSchema,
  sourceType: knowledgeSourceTypeSchema,
  sourceId: z.string().uuid().nullable(),
  includedAt: z.string(),
});

export const knowledgeUsageListSchema = z.object({
  items: z.array(knowledgeUsageSchema),
});

export type KnowledgeStatus = z.infer<typeof knowledgeStatusSchema>;
export type KnowledgeScope = z.infer<typeof knowledgeScopeSchema>;
export type KnowledgeType = z.infer<typeof knowledgeTypeSchema>;
export type KnowledgeSourceType = z.infer<typeof knowledgeSourceTypeSchema>;
export type KnowledgeConfidence = z.infer<typeof knowledgeConfidenceSchema>;
export type KnowledgeItem = z.infer<typeof knowledgeItemSchema>;
export type KnowledgePage = z.infer<typeof knowledgePageSchema>;
export type KnowledgeUsage = z.infer<typeof knowledgeUsageSchema>;
