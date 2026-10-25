import { z } from 'zod';
import { knowledgeItemSchema } from './knowledge';

export const chatSessionStatusSchema = z.enum(['active', 'archived', 'cutoff']);

export const chatMessageRoleSchema = z.enum(['human', 'agent', 'system']);

export const chatSessionSchema = z.object({
  id: z.string().uuid(),
  boardId: z.string().uuid().nullable(),
  ownerUserId: z.string().uuid(),
  agentId: z.string().uuid(),
  repoId: z.string().uuid().nullable(),
  parentSessionId: z.string().uuid().nullable().optional(),
  status: chatSessionStatusSchema,
  lastMessagePreview: z.string().default(''),
  lastMessageAt: z.string().nullable().optional(),
  lastMessageRole: chatMessageRoleSchema.nullable().optional(),
  hasActiveRun: z.boolean().default(false),
  activeRunId: z.string().uuid().nullable().optional(),
  title: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const chatActionMetadataSchema = z.object({
  action: z.enum(['create_ticket', 'create_knowledge']),
  ticketId: z.string().uuid().optional(),
  knowledgeItemId: z.string().uuid().optional(),
});

export const chatProposalSchema = z.object({
  id: z.string().uuid(),
  type: z.enum(['create_ticket', 'create_knowledge']),
  status: z.enum(['pending', 'created', 'rejected']),
  title: z.string(),
  description: z.string().optional(),
  content: z.string().optional(),
  boardId: z.string().uuid().nullable().optional(),
  repoId: z.string().uuid().nullable().optional(),
  knowledgeType: z.string().optional(),
  scope: z.string().optional(),
  ticketId: z.string().uuid().optional(),
  knowledgeItemId: z.string().uuid().optional(),
});

const chatProposalMetadataSchema = z.object({
  proposals: z.array(chatProposalSchema),
});

export type ChatProposal = z.infer<typeof chatProposalSchema>;

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
  /** Pass as beforeSeq to load older messages. Absent at the start of the chat. */
  olderBeforeSeq: z.number().int().optional(),
});

export type ChatMessagePage = z.infer<typeof chatMessageListSchema>;

export const postChatMessageResponseSchema = z.object({
  message: chatMessageSchema,
  runId: z.string().uuid(),
});

/** Ticket fields needed after accepting a proposal; full board shape varies. */
const chatCreatedTicketSchema = z
  .object({
    id: z.string().uuid(),
    boardId: z.string().uuid(),
    title: z.string(),
    status: z.string(),
  })
  .passthrough();

export const acceptChatProposalResponseSchema = z.object({
  message: chatMessageSchema,
  ticket: chatCreatedTicketSchema.optional(),
  knowledge: knowledgeItemSchema.optional(),
});

export type ChatSession = z.infer<typeof chatSessionSchema>;
export type ChatMessage = z.infer<typeof chatMessageSchema>;
export type ChatAttachment = z.infer<typeof chatAttachmentSchema>;
export type ChatSessionStatus = z.infer<typeof chatSessionStatusSchema>;
export type ChatMessageRole = z.infer<typeof chatMessageRoleSchema>;
export type ChatActionMetadata = z.infer<typeof chatActionMetadataSchema>;
export type AcceptChatProposalResponse = z.infer<
  typeof acceptChatProposalResponseSchema
>;

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

/** Pending or settled proposals on an agent message; invalid metadata yields none. */
export function parseChatProposals(value: unknown): ChatProposal[] {
  const parsed = chatProposalMetadataSchema.safeParse(value);
  return parsed.success ? parsed.data.proposals : [];
}
