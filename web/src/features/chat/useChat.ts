import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import {
  acceptChatProposalResponseSchema,
  chatMessageListSchema,
  chatMessageSchema,
  chatSessionListSchema,
  chatSessionSchema,
  postChatMessageResponseSchema,
  type AcceptChatProposalResponse,
  type ChatMessage,
  type ChatMessagePage,
  type ChatSession,
  type ChatSessionStatus,
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
  boardId?: string | null;
  repoId?: string | null;
}

export interface PostChatMessageResult {
  message: ChatMessage;
  runId: string;
}

export interface AcceptChatProposalInput {
  messageId: string;
  proposalId: string;
  title?: string;
  /** Ticket description, or knowledge content. */
  description?: string;
  boardId?: string;
  knowledgeType?: KnowledgeType;
  scope?: KnowledgeScope;
}

async function fetchSessions(boardId?: string | null): Promise<ChatSession[]> {
  const params = new URLSearchParams();
  if (boardId) params.set('boardId', boardId);
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
      boardId: input.boardId ?? null,
      repoId: input.repoId ?? null,
    }),
  });
  return chatSessionSchema.parse(await res.json());
}

/** The newest page of a chat, oldest first, with the cursor for older ones. */
async function fetchMessages(sessionId: string): Promise<ChatMessagePage> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/messages`);
  return chatMessageListSchema.parse(await res.json());
}

/** Messages before `beforeSeq`, oldest first. */
export async function fetchOlderMessages(
  sessionId: string,
  beforeSeq: number,
): Promise<ChatMessagePage> {
  const res = await apiFetch(`/api/chat/sessions/${sessionId}/messages?beforeSeq=${beforeSeq}`);
  return chatMessageListSchema.parse(await res.json());
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

function proposalPath(sessionId: string, messageId: string, proposalId: string) {
  return `/api/chat/sessions/${sessionId}/messages/${messageId}/proposals/${proposalId}`;
}

async function acceptProposal(
  sessionId: string,
  input: AcceptChatProposalInput,
): Promise<AcceptChatProposalResponse> {
  const res = await apiFetch(
    `${proposalPath(sessionId, input.messageId, input.proposalId)}/accept`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title: input.title,
        description: input.description,
        boardId: input.boardId,
        knowledgeType: input.knowledgeType,
        scope: input.scope,
      }),
    },
  );
  return acceptChatProposalResponseSchema.parse(await res.json());
}

async function rejectProposal(
  sessionId: string,
  messageId: string,
  proposalId: string,
): Promise<ChatMessage> {
  const res = await apiFetch(
    `${proposalPath(sessionId, messageId, proposalId)}/reject`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    },
  );
  return chatMessageSchema.parse(await res.json());
}

export function useChatSessions(boardId?: string | null) {
  return useQuery({
    queryKey: [...CHAT_SESSIONS_QUERY_KEY, boardId ?? null],
    queryFn: () => fetchSessions(boardId),
    refetchInterval: (query) =>
      query.state.data?.some((session) => session.hasActiveRun) ? 3000 : false,
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
  opts?: { refetchInterval?: number | false | (() => number | false) },
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
    mutationFn: (input: PostChatMessageInput & { sessionId?: string }) => {
      const { sessionId: override, ...message } = input;
      return postMessage(override ?? sessionId, message);
    },
    onSuccess: (_result, input) => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(input.sessionId ?? sessionId),
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

export function useAcceptChatProposal(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: AcceptChatProposalInput) =>
      acceptProposal(sessionId, input),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
      void queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_QUERY_KEY });
      if (result.ticket) {
        void queryClient.invalidateQueries({
          queryKey: ['tickets', result.ticket.boardId],
        });
      }
      if (result.knowledge) {
        void queryClient.invalidateQueries({ queryKey: KNOWLEDGE_QUERY_KEY });
      }
    },
  });
}

export function useRejectChatProposal(sessionId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { messageId: string; proposalId: string }) =>
      rejectProposal(sessionId, input.messageId, input.proposalId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: chatMessagesQueryKey(sessionId),
      });
    },
  });
}

