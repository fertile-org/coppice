import { z } from 'zod';
import { knowledgeItemSchema } from './knowledge';

export const chatSessionStatusSchema = z.enum(['active', 'archived', 'cutoff']);

export const chatMessageRoleSchema = z.enum(['human', 'agent', 'system']);

export const chatSessionSchema = z.object({
  id: z.string().uuid(),
  projectId: z.string().uuid().nullable(),
  ownerUserId: z.string().uuid(),
  agentId: z.string().uuid(),
  repoId: z.string().uuid().nullable(),
  parentSessionId: z.string().uuid().nullable().optional(),
  status: chatSessionStatusSchema,
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const chatActionMetadataSchema = z.object({
  action: z.enum([
    'create_ticket',
    'create_knowledge',
    'cutoff',
    'cutoff_seed',
  ]),
  ticketId: z.string().uuid().optional(),
  knowledgeItemId: z.string().uuid().optional(),
  childSessionId: z.string().uuid().optional(),
  parentSessionId: z.string().uuid().optional(),
});

export const chatAttachmentSchema = z.object({
  id: z.string().uuid(),
  filename: z.string(),
  contentType: z.string(),
  sizeBytes: z.number().int(),
});

export const chatMessageSchema = z.object({
  id: z.string().uuid(),
  sessionId: z.string().uuid(),
  seq: z.number().int(),
  role: chatMessageRoleSchema,
  body: z.string(),
  agentRunId: z.string().uuid().nullable(),
  actionMetadata: z.unknown().nullable(),
  attachmentIds: z.array(z.string().uuid()).default([]),
  attachments: z.array(chatAttachmentSchema).default([]),
  createdAt: z.string(),
});

export const chatSessionListSchema = z.object({
  sessions: z.array(chatSessionSchema),
});

export const chatMessageListSchema = z.object({
  messages: z.array(chatMessageSchema),
});

export const postChatMessageResponseSchema = z.object({
  message: chatMessageSchema,
  runId: z.string().uuid(),
});

/** Ticket fields needed after create-ticket; full board shape varies. */
const chatCreatedTicketSchema = z
  .object({
    id: z.string().uuid(),
    projectId: z.string().uuid(),
    title: z.string(),
    status: z.string(),
  })
  .passthrough();

export const createTicketFromChatResponseSchema = z.object({
  ticket: chatCreatedTicketSchema,
  message: chatMessageSchema,
});

export const createKnowledgeFromChatResponseSchema = z.object({
  knowledge: knowledgeItemSchema,
  message: chatMessageSchema,
});

export const cutoffSessionResponseSchema = z.object({
  parent: chatSessionSchema,
  child: chatSessionSchema,
  seedMessage: chatMessageSchema,
});

export type ChatSession = z.infer<typeof chatSessionSchema>;
export type ChatMessage = z.infer<typeof chatMessageSchema>;
export type ChatAttachment = z.infer<typeof chatAttachmentSchema>;
export type ChatSessionStatus = z.infer<typeof chatSessionStatusSchema>;
export type ChatMessageRole = z.infer<typeof chatMessageRoleSchema>;
export type ChatActionMetadata = z.infer<typeof chatActionMetadataSchema>;
export type CreateTicketFromChatResponse = z.infer<
  typeof createTicketFromChatResponseSchema
>;
export type CreateKnowledgeFromChatResponse = z.infer<
  typeof createKnowledgeFromChatResponseSchema
>;
export type CutoffSessionResponse = z.infer<typeof cutoffSessionResponseSchema>;

export interface PostChatMessageInput {
  body: string;
  attachmentIds?: string[];
}

export function parseChatActionMetadata(
  value: unknown,
): ChatActionMetadata | null {
  const parsed = chatActionMetadataSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}
