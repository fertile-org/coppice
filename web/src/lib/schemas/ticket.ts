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

export const createCommentSchema = z.object({
  body: z.string().min(1, 'Comment cannot be empty'),
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
