import { describe, expect, it } from 'vitest';
import {
  chatActionMetadataSchema,
  chatMessageListSchema,
  chatMessageSchema,
  chatSessionListSchema,
  chatSessionSchema,
  createKnowledgeFromChatResponseSchema,
  createTicketFromChatResponseSchema,
  cutoffSessionResponseSchema,
  parseChatActionMetadata,
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

const knowledgeItem = {
  id: '00000000-0000-4000-8000-000000000080',
  version: 1,
  status: 'pending',
  revisionId: '00000000-0000-4000-8000-000000000081',
  revisionNumber: 1,
  activeRevisionId: null,
  scope: 'project',
  projectId: '00000000-0000-4000-8000-000000000003',
  projectName: 'Coppice',
  agentId: null,
  agentName: null,
  knowledgeType: 'coding_convention',
  title: 'Chat cwd rule',
  content: 'Resolve under worktrees',
  sourceType: 'chat_session',
  sourceId: '00000000-0000-4000-8000-000000000001',
  sourceRunId: null,
  confidence: 'medium',
  approvedBy: null,
  approvedAt: null,
  approvalMode: null,
  policyDecision: null,
  policyReason: null,
  rejectionReason: null,
  expiresAt: null,
  supersedesItemId: null,
  supersededBy: null,
  staleAt: null,
  embeddingStatus: 'pending',
  embeddingError: null,
  usageCount: 0,
  lastUsedAt: null,
  createdAt: '2026-09-08T00:00:00Z',
  updatedAt: '2026-09-08T00:00:00Z',
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

  it('parses action metadata and human action responses', () => {
    expect(
      chatActionMetadataSchema.parse({
        action: 'create_ticket',
        ticketId: '00000000-0000-4000-8000-000000000050',
      }).ticketId,
    ).toBe('00000000-0000-4000-8000-000000000050');
    expect(parseChatActionMetadata({ action: 'nope' })).toBeNull();

    expect(
      createTicketFromChatResponseSchema.parse({
        ticket: {
          id: '00000000-0000-4000-8000-000000000050',
          projectId: '00000000-0000-4000-8000-000000000003',
          title: 'Fix chat cwd',
          status: 'backlog',
          hasActiveRun: false,
          repoId: null,
        },
        message: {
          ...message,
          role: 'system',
          actionMetadata: {
            action: 'create_ticket',
            ticketId: '00000000-0000-4000-8000-000000000050',
          },
        },
      }).ticket.title,
    ).toBe('Fix chat cwd');

    expect(
      createKnowledgeFromChatResponseSchema.parse({
        knowledge: knowledgeItem,
        message: {
          ...message,
          role: 'system',
          actionMetadata: {
            action: 'create_knowledge',
            knowledgeItemId: knowledgeItem.id,
          },
        },
      }).knowledge.sourceType,
    ).toBe('chat_session');

    const cutoff = cutoffSessionResponseSchema.parse({
      parent: { ...session, status: 'cutoff' },
      child: {
        ...session,
        id: '00000000-0000-4000-8000-000000000070',
        parentSessionId: session.id,
      },
      seedMessage: {
        ...message,
        sessionId: '00000000-0000-4000-8000-000000000070',
        role: 'system',
        body: 'Summary',
        actionMetadata: {
          action: 'cutoff_seed',
          parentSessionId: session.id,
        },
      },
    });
    expect(cutoff.parent.status).toBe('cutoff');
    expect(cutoff.child.parentSessionId).toBe(session.id);
  });
});
