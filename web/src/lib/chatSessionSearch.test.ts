import { describe, expect, it } from 'vitest';
import { filterChatSessions } from './chatSessionSearch';
import type { ChatSession } from './schemas/chat';

const session: ChatSession = {
  id: '00000000-0000-4000-8000-000000000001',
  projectId: null,
  ownerUserId: '00000000-0000-4000-8000-000000000002',
  agentId: '00000000-0000-4000-8000-000000000010',
  repoId: null,
  status: 'active',
  lastMessagePreview: 'cwd policy question',
  hasActiveRun: false,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};

describe('filterChatSessions', () => {
  it('matches agent name and preview text', () => {
    const map = new Map([
      ['00000000-0000-4000-8000-000000000010', 'Backend Engineer'],
    ]);
    expect(filterChatSessions([session], map, 'backend')).toHaveLength(1);
    expect(filterChatSessions([session], map, 'cwd')).toHaveLength(1);
    expect(filterChatSessions([session], map, 'nomatch')).toHaveLength(0);
  });
});
