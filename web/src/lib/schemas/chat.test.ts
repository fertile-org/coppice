import { describe, expect, it } from 'vitest';
import {
  chatMessageListSchema,
  chatMessageSchema,
  chatSessionListSchema,
  chatSessionSchema,
  postChatMessageResponseSchema,
} from './chat';

const session = {
  id: '00000000-0000-4000-8000-000000000001',
  projectId: null,
  ownerUserId: '00000000-0000-4000-8000-000000000002',
  agentId: '00000000-0000-4000-8000-000000000003',
  repoId: null,
  status: 'active',
  createdAt: '2026-09-08T00:00:00Z',
  updatedAt: '2026-09-08T00:00:00Z',
};

const message = {
  id: '00000000-0000-4000-8000-000000000010',
  sessionId: '00000000-0000-4000-8000-000000000001',
  seq: 1,
  role: 'human',
  body: 'Hello',
  agentRunId: '00000000-0000-4000-8000-000000000020',
  actionMetadata: null,
  createdAt: '2026-09-08T00:01:00Z',
};

describe('chat schemas', () => {
  it('parses a session and session list', () => {
    expect(chatSessionSchema.parse(session).status).toBe('active');
    expect(chatSessionListSchema.parse({ sessions: [session] }).sessions).toHaveLength(1);
  });

  it('parses messages and post-message responses', () => {
    expect(chatMessageSchema.parse(message).role).toBe('human');
    expect(
      chatMessageListSchema.parse({ messages: [message] }).messages[0]?.body,
    ).toBe('Hello');
    expect(
      postChatMessageResponseSchema.parse({
        message,
        runId: '00000000-0000-4000-8000-000000000020',
      }).runId,
    ).toBe('00000000-0000-4000-8000-000000000020');
  });

  it('rejects unknown session status', () => {
    expect(() =>
      chatSessionSchema.parse({ ...session, status: 'paused' }),
    ).toThrow();
  });
});
