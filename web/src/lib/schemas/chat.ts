import { z } from 'zod';

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

export const chatMessageSchema = z.object({
  id: z.string().uuid(),
  sessionId: z.string().uuid(),
  seq: z.number().int(),
  role: chatMessageRoleSchema,
  body: z.string(),
  agentRunId: z.string().uuid().nullable(),
  actionMetadata: z.unknown().nullable(),
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

export type ChatSession = z.infer<typeof chatSessionSchema>;
export type ChatMessage = z.infer<typeof chatMessageSchema>;
export type ChatSessionStatus = z.infer<typeof chatSessionStatusSchema>;
export type ChatMessageRole = z.infer<typeof chatMessageRoleSchema>;
