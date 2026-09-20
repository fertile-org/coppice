import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  chatMessageListSchema,
  chatSessionListSchema,
  chatSessionSchema,
  createKnowledgeFromChatResponseSchema,
  createTicketFromChatResponseSchema,
  cutoffSessionResponseSchema,
  postChatMessageResponseSchema,
  type ChatMessage,
  type ChatSession,
  type ChatSessionStatus,
  type CreateKnowledgeFromChatResponse,
  type CreateTicketFromChatResponse,
  type CutoffSessionResponse,
  type PostChatMessageInput,
} from '../../lib/schemas/chat';
import type {
  KnowledgeScope,
  KnowledgeType,
} from '../../lib/schemas/knowledge';
import { KNOWLEDGE_QUERY_KEY } from '../knowledge/useKnowledge';

export const CHAT_SESSIONS_QUERY_KEY = ['chat-sessions'] as const;

export function chatMessagesQueryKey(sessionId: string) {
  return ['chat-messages', sessionId] as const;
}

export interface CreateChatSessionInput {
  agentId: string;
  projectId?: string | null;
  repoId?: string | null;
}

export interface PostChatMessageResult {
  message: ChatMessage;
  runId: string;
}

export interface CreateTicketFromChatInput {
  projectId: string;
  title?: string;
  description?: string;
  repoId?: string | null;
}

export interface CreateKnowledgeFromChatInput {
  title?: string;
  content?: string;
  knowledgeType?: KnowledgeType;
  scope?: KnowledgeScope;
  projectId?: string;
}

async function fetchSessions(projectId?: string | null): Promise<ChatSession[]> {
  const params = new URLSearchParams();
  if (projectId) params.set('projectId', projectId);
  const qs = params.toString();
  const res = await apiFetch(`/api/chat/sessions${qs ? `?${qs}` : ''}`);
  return chatSessionListSchema.parse(await res.json()).sessions;
}

async function fetchSession(sessionId: string): Promise<ChatSession> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}`);
  return chatSessionSchema.parse(await res.json());
}

async function createSession(input: CreateChatSessionInput): Promise<ChatSession> {
  const res = await apiFetch('/api/chat/sessions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      agentId: input.agentId,
      projectId: input.projectId ?? null,
      repoId: input.repoId ?? null,
    }),
  });
  return chatSessionSchema.parse(await res.json());
}

async function fetchMessages(sessionId: string): Promise<ChatMessage[]> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/messages`);
  return chatMessageListSchema.parse(await res.json()).messages;
}

async function postMessage(
  sessionId: string,
  input: PostChatMessageInput,
): Promise<PostChatMessageResult> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      body: input.body,
      attachmentIds:
        input.attachmentIds && input.attachmentIds.length > 0
          ? input.attachmentIds
          : undefined,
    }),
  });
  return postChatMessageResponseSchema.parse(await res.json());
}

async function patchSessionStatus(
  sessionId: string,
  status: ChatSessionStatus,
): Promise<ChatSession> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ status }),
  });
  return chatSessionSchema.parse(await res.json());
}

async function createTicketFromChat(
  sessionId: string,
  input: CreateTicketFromChatInput,
): Promise<CreateTicketFromChatResponse> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/create-ticket`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      projectId: input.projectId,
      title: input.title,
      description: input.description,
      repoId: input.repoId ?? undefined,
    }),
  });
  return createTicketFromChatResponseSchema.parse(await res.json());
}

async function createKnowledgeFromChat(
  sessionId: string,
  input: CreateKnowledgeFromChatInput,
): Promise<CreateKnowledgeFromChatResponse> {
  const res = await apiFetch(
    `/api/chat/sessions/${sessionId}/create-knowledge`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title: input.title,
        content: input.content,
        knowledgeType: input.knowledgeType,
        scope: input.scope,
        projectId: input.projectId,
      }),
    },
  );
  return createKnowledgeFromChatResponseSchema.parse(await res.json());
}

async function cutoffSession(sessionId: string): Promise<CutoffSessionResponse> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/cutoff`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
  });
  return cutoffSessionResponseSchema.parse(await res.json());
}

export function useChatSessions(projectId?: string | null) {
  return useQuery({
    queryKey: [...CHAT_SESSIONS_QUERY_KEY, projectId ?? null],
    queryFn: () => fetchSessions(projectId),
  });
}

export function useChatSession(sessionId: string | null) {
  return useQuery({
    queryKey: [...CHAT_SESSIONS_QUERY_KEY, 'detail', sessionId],
    queryFn: () => fetchSession(sessionId!),
    enabled: Boolean(sessionId),
  });
}

export function useChatMessages(
  sessionId: string | null,
  opts?: { refetchInterval?: number | false },
) {
  return useQuery({
    queryKey: sessionId
      ? chatMessagesQueryKey(sessionId)
      : ['chat-messages', 'none'],
    queryFn: () => fetchMessages(sessionId!),
    enabled: Boolean(sessionId),
    refetchInterval: opts?.refetchInterval,
  });
}

export function useCreateChatSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createSession,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
    },
  });
}

export function usePostChatMessage(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: PostChatMessageInput) => postMessage(sessionId, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
    },
  });
}

export function usePatchChatSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      sessionId,
      status,
    }: {
      sessionId: string;
      status: ChatSessionStatus;
    }) => patchSessionStatus(sessionId, status),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
    },
  });
}

export function useCreateTicketFromChat(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateTicketFromChatInput) =>
      createTicketFromChat(sessionId, input),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
      void queryClient.invalidateQueries({
        queryKey: ['tickets', result.ticket.projectId],
      });
    },
  });
}

export function useCreateKnowledgeFromChat(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateKnowledgeFromChatInput) =>
      createKnowledgeFromChat(sessionId, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: KNOWLEDGE_QUERY_KEY });
    },
  });
}

export function useCutoffChatSession(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => cutoffSession(sessionId),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(result.child.id),
      });
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
    },
  });
}
