import { z } from 'zod';
import { substatusSchema } from './substatus';

export const ticketStatusSchema = z.enum([
  'backlog',
  'ready',
  'in_progress',
  'in_review',
  'in_qa',
  'wait_for_final_review',
  'done',
  'blocked',
]);

export type TicketStatusValue = z.infer<typeof ticketStatusSchema>;

export const ticketPrioritySchema = z.enum(['low', 'medium', 'high', 'critical']);

export type TicketPriorityValue = z.infer<typeof ticketPrioritySchema>;

export const updateTicketSchema = z.object({
  title: z.string().min(1, 'Title is required').optional(),
  description: z.string().optional(),
  repoId: z.string().uuid().optional().nullable(),
  priority: ticketPrioritySchema.optional().nullable(),
  branchName: z.string().optional().nullable(),
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
  attachmentIds: z.array(z.string().uuid()).optional(),
});

export type CreateCommentInput = z.infer<typeof createCommentSchema>;

export const pendingRecommendationSchema = z.object({
  recommendedAgentKey: z.string(),
  recommendedByAgentId: z.string().uuid(),
  recommendedAt: z.string(),
  summary: z.string().optional(),
});

export type PendingRecommendation = z.infer<typeof pendingRecommendationSchema>;

export const splitTicketSpecSchema = z.object({
  title: z.string(),
  description: z.string(),
  acceptanceCriteria: z.string().optional(),
  assignTo: z.string().optional(),
});

export type SplitTicketSpec = z.infer<typeof splitTicketSpecSchema>;

export const pendingSplitRecommendationSchema = z.object({
  recommendedByAgentId: z.string().uuid(),
  recommendedAt: z.string(),
  splits: z.array(splitTicketSpecSchema),
});

export type PendingSplitRecommendation = z.infer<
  typeof pendingSplitRecommendationSchema
>;

export const ticketSchema = z.object({
  id: z.string().uuid(),
  projectId: z.string().uuid(),
  repoId: z.string().uuid().optional(),
  title: z.string(),
  description: z.string(),
  status: ticketStatusSchema,
  substatus: z.string().optional(),
  substatusMetadata: z.record(z.unknown()).optional(),
  priority: ticketPrioritySchema.optional(),
  assigneeAgentId: z.string().uuid().optional(),
  ownerUserId: z.string().uuid().optional(),
  branchName: z.string().optional(),
  createdBy: z.string(),
  createdById: z.string().uuid().optional(),
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
  parentTicketId: z.string().uuid().nullable().optional(),
  pendingSplitRecommendation: pendingSplitRecommendationSchema
    .nullable()
    .optional(),
  clarificationRound: z.number().optional(),
});

export type TicketResponse = z.infer<typeof ticketSchema>;
