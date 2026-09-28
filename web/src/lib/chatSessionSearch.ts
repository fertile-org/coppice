import type { ChatSession } from './schemas/chat';

export function filterChatSessions(
  sessions: ChatSession[],
  agentNameById: Map<string, string>,
  query: string,
): ChatSession[] {
  const q = query.trim().toLowerCase();
  if (!q) return sessions;
  return sessions.filter((session) => {
    const name = (agentNameById.get(session.agentId) ?? '').toLowerCase();
    const preview = session.lastMessagePreview.toLowerCase();
    return name.includes(q) || preview.includes(q);
  });
}
