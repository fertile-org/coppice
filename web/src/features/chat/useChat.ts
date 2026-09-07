import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  chatMessageListSchema,
  chatSessionListSchema,
  chatSessionSchema,
  postChatMessageResponseSchema,
  type ChatMessage,
  type ChatSession,
  type ChatSessionStatus,
} from '../../lib/schemas/chat';

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
  body: string,
): Promise<PostChatMessageResult> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ body }),
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
    mutationFn: (body: string) => postMessage(sessionId, body),
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
