import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { usageGroupSchema, type UsageGroup } from '../../lib/schemas/usage';

export function chatSessionUsageQueryKey(sessionId: string) {
  return ['chat-session-usage', sessionId] as const;
}

async function fetchChatSessionUsage(sessionId: string): Promise<UsageGroup> {
  const response = await apiFetch(`/api/chat/sessions/${sessionId}/usage`);
  return usageGroupSchema.parse(await response.json());
}

/** The session's turns added up. Refetched when a turn finishes. */
export function useChatSessionUsage(sessionId: string | null) {
  return useQuery({
    queryKey: chatSessionUsageQueryKey(sessionId ?? ''),
    queryFn: () => fetchChatSessionUsage(sessionId ?? ''),
    enabled: Boolean(sessionId),
  });
}
