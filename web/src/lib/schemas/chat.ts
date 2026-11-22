import { z } from 'zod';
import { knowledgeItemSchema } from './knowledge';
import { idSchema } from './id';

export const chatSessionStatusSchema = z.enum(['active', 'archived', 'cutoff']);

export const chatMessageRoleSchema = z.enum(['human', 'agent', 'system']);

export const chatSessionSchema = z.object({
  id: idSchema,
  boardId: idSchema.nullable(),
  ownerUserId: idSchema,
  agentId: idSchema,
  repoId: idSchema.nullable(),
  parentSessionId: idSchema.nullable().optional(),
  status: chatSessionStatusSchema,
  lastMessagePreview: z.string().default(''),
  lastMessageAt: z.string().nullable().optional(),
  lastMessageRole: chatMessageRoleSchema.nullable().optional(),
  hasActiveRun: z.boolean().default(false),
  activeRunId: idSchema.nullable().optional(),
  title: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const chatActionMetadataSchema = z.object({
  action: z.enum(['create_ticket', 'create_knowledge']),
  ticketId: idSchema.optional(),
  knowledgeItemId: idSchema.optional(),
});

export const chatProposalChildSchema = z.object({
  id: idSchema,
  title: z.string(),
  description: z.string().optional(),
});

export const chatProposalSchema = z.object({
  id: idSchema,
  type: z.enum(['create_ticket', 'create_knowledge']),
  status: z.enum(['pending', 'created', 'rejected']),
  title: z.string(),
  description: z.string().optional(),
  content: z.string().optional(),
  boardId: idSchema.nullable().optional(),
  repoId: idSchema.nullable().optional(),
  knowledgeType: z.string().optional(),
  scope: z.string().optional(),
  children: z.array(chatProposalChildSchema).optional(),
  designSetId: idSchema.nullable().optional(),
  ticketId: idSchema.optional(),
  childTicketIds: z.array(idSchema).optional(),
  knowledgeItemId: idSchema.optional(),
});

const chatProposalMetadataSchema = z.object({
  proposals: z.array(chatProposalSchema),
});

export type ChatProposal = z.infer<typeof chatProposalSchema>;

export const chatAttachmentSchema = z.object({
  id: idSchema,
  filename: z.string(),
  contentType: z.string(),
  sizeBytes: z.number().int(),
});

export const chatMessageSchema = z.object({
  id: idSchema,
  sessionId: idSchema,
  seq: z.number().int(),
  role: chatMessageRoleSchema,
  body: z.string(),
  agentRunId: idSchema.nullable(),
  actionMetadata: z.unknown().nullable(),
  attachmentIds: z.array(idSchema).default([]),
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
  runId: idSchema,
});

/** Ticket fields needed after accepting a proposal; full board shape varies. */
const chatCreatedTicketSchema = z
  .object({
    id: idSchema,
    boardId: idSchema,
    title: z.string(),
    status: z.string(),
  })
  .passthrough();

export const acceptChatProposalResponseSchema = z.object({
  message: chatMessageSchema,
  ticket: chatCreatedTicketSchema.optional(),
  children: z.array(chatCreatedTicketSchema).optional(),
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
