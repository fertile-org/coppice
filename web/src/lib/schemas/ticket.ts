import { z } from 'zod';
import { substatusSchema } from './substatus';
import { idSchema } from './id';

export const ticketStatusSchema = z.enum([
  'backlog',
  'ready',
  'plan_review',
  'in_progress',
  'in_review',
  'in_qa',
  'wait_for_final_review',
  'done',
]);

export type TicketStatusValue = z.infer<typeof ticketStatusSchema>;

export const ticketPrioritySchema = z.enum(['low', 'medium', 'high', 'critical']);

export type TicketPriorityValue = z.infer<typeof ticketPrioritySchema>;

export const updateTicketSchema = z.object({
  title: z.string().min(1, 'Title is required').optional(),
  description: z.string().optional(),
  attachmentIds: z.array(idSchema).optional(),
  repoId: idSchema.optional().nullable(),
  priority: ticketPrioritySchema.optional().nullable(),
  branchName: z.string().optional().nullable(),
  skipPlanning: z.boolean().optional(),
  skipHumanReview: z.boolean().optional(),
});

export type UpdateTicketInput = z.infer<typeof updateTicketSchema>;

export const updateStatusSchema = z.object({
  status: ticketStatusSchema,
  substatus: substatusSchema.optional().nullable(),
  substatusMetadata: z.record(z.unknown()).optional(),
});

export type UpdateStatusInput = z.infer<typeof updateStatusSchema>;

export const mentionModeSchema = z.enum(['agent', 'chat']);
export type MentionMode = z.infer<typeof mentionModeSchema>;

export const createCommentSchema = z.object({
  body: z.string().min(1, 'Comment cannot be empty'),
  mentionMode: mentionModeSchema.optional(),
  intent: z
    .enum([
      'progress_update',
      'clarification_request',
      'clarification_answer',
      'review_feedback',
      'bug_report',
      'implementation_done',
      'qa_failed',
      'qa_passed',
      'blocked',
      'system_event',
    ])
    .optional(),
  attachmentIds: z.array(idSchema).optional(),
});

export type CreateCommentInput = z.infer<typeof createCommentSchema>;

export const pendingRecommendationSchema = z.object({
  recommendedAgentKey: z.string(),
  recommendedByAgentId: idSchema,
  recommendedAt: z.string(),
  summary: z.string().optional(),
});

export type PendingRecommendation = z.infer<typeof pendingRecommendationSchema>;

export const splitTicketSpecSchema = z.object({
  title: z.string(),
  description: z.string(),
  acceptanceCriteria: z.string().optional(),
  assignTo: z.string().optional(),
  steps: z.array(z.string()).optional(),
});

export type SplitTicketSpec = z.infer<typeof splitTicketSpecSchema>;

export const pendingSplitRecommendationSchema = z.object({
  recommendedByAgentId: idSchema,
  recommendedAt: z.string(),
  splits: z.array(splitTicketSpecSchema),
  /** Proposed with a plan: created only by approving that plan. */
  fromPlan: z.boolean().optional(),
});

export type PendingSplitRecommendation = z.infer<
  typeof pendingSplitRecommendationSchema
>;

export const ticketSchema = z.object({
  id: idSchema,
  boardId: idSchema,
  repoId: idSchema.optional(),
  title: z.string(),
  description: z.string(),
  status: ticketStatusSchema,
  substatus: z.string().optional(),
  substatusMetadata: z.record(z.unknown()).optional(),
  priority: ticketPrioritySchema.optional(),
  assigneeAgentId: idSchema.optional(),
  suggestedAssignee: z
    .object({
      id: idSchema,
      name: z.string(),
    })
    .nullable()
    .optional(),
  ownerUserId: idSchema.optional(),
  branchName: z.string().optional(),
  createdBy: z.string(),
  createdById: idSchema.optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
  lastActivityAt: z.string(),
  substatusDisplay: z
    .object({
      label: z.string(),
      detail: z.string().optional(),
    })
    .optional(),
  pendingAssignRecommendation: pendingRecommendationSchema.nullable().optional(),
  parentTicketId: idSchema.nullable().optional(),
  pendingSplitRecommendation: pendingSplitRecommendationSchema
    .nullable()
    .optional(),
  clarificationRound: z.number().optional(),
  skipPlanning: z.boolean().optional(),
  skipHumanReview: z.boolean().optional(),
  planApproved: z.boolean().optional(),
  latestPlanRunStatus: z.string().nullable().optional(),
  hasActiveRun: z.boolean().optional(),
  archivedAt: z.string().nullable().optional(),
  attachmentIds: z.array(idSchema).optional(),
  attachments: z
    .array(
      z.object({
        id: idSchema,
        filename: z.string(),
        contentType: z.string(),
        sizeBytes: z.number(),
      }),
    )
    .optional(),
  humanReview: z
    .object({
      headSha: z.string(),
      shortSha: z.string(),
      stale: z.boolean(),
      merged: z.boolean().optional(),
      commentIds: z.array(idSchema),
      runIds: z.array(idSchema),
    })
    .nullable()
    .optional(),
});

export type TicketResponse = z.infer<typeof ticketSchema>;
