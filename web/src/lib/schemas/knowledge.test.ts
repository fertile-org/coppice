import { describe, expect, it } from 'vitest';
import {
  compactionStatusSchema,
  knowledgeItemSchema,
  knowledgeUsageListSchema,
  similarListSchema,
} from './knowledge';

const ID = '00000000-0000-4000-8000-000000000001';
const REVISION_ID = '00000000-0000-4000-8000-000000000002';

describe('knowledge schemas', () => {
  it('parses similar-neighbor assist payloads', () => {
    const parsed = similarListSchema.parse({
      items: [
        {
          itemId: ID,
          revisionId: REVISION_ID,
          title: 'Existing rule',
          knowledgeType: 'test_command',
          scope: 'board',
          boardId: '00000000-0000-4000-8000-000000000003',
          score: 0.97,
          status: 'approved',
        },
      ],
    });
    expect(parsed.items).toHaveLength(1);
    expect(parsed.items[0].score).toBe(0.97);
    expect(similarListSchema.parse({ items: [] }).items).toEqual([]);
  });

  it('parses lifecycle, provenance, compaction, and usage metadata', () => {
    const parsed = knowledgeItemSchema.parse({
      id: ID,
      version: 3,
      status: 'approved',
      revisionId: REVISION_ID,
      revisionNumber: 2,
      activeRevisionId: REVISION_ID,
      scope: 'board',
      boardId: '00000000-0000-4000-8000-000000000003',
      boardName: 'Coppice',
      agentId: null,
      agentName: null,
      knowledgeType: 'test_command',
      title: 'Fast tests',
      content: 'Run make test-unit while iterating.',
      sourceType: 'human_note',
      sourceId: null,
      sourceRunId: null,
      confidence: 'high',
      approvedBy: '00000000-0000-4000-8000-000000000004',
      approvedAt: '2026-08-03T12:00:00Z',
      approvalMode: 'human',
      policyDecision: null,
      policyReason: null,
      rejectionReason: null,
      expiresAt: null,
      supersedesItemId: null,
      supersededBy: null,
      staleAt: null,
      compactionBatchId: '00000000-0000-4000-8000-000000000005',
      compactionAgentId: '00000000-0000-4000-8000-000000000006',
      compactionAgentName: 'Reviewer',
      sourceTicketIds: ['00000000-0000-4000-8000-000000000007'],
      usageCount: 4,
      lastUsedAt: '2026-08-03T12:30:00Z',
      createdAt: '2026-08-03T11:00:00Z',
      updatedAt: '2026-08-03T12:00:00Z',
    });

    expect(parsed.revisionId).toBe(REVISION_ID);
    expect(parsed.usageCount).toBe(4);
    expect(parsed.sourceTicketIds).toHaveLength(1);
  });

  it('rejects unknown types and preserves the exact used revision', () => {
    expect(() =>
      knowledgeUsageListSchema.parse({
        items: [
          {
            itemId: ID,
            revisionId: REVISION_ID,
            rank: 1,
            score: 0.91,
            tokenCount: 12,
            renderedContent: '<knowledge>exact revision</knowledge>',
            title: 'Fast tests',
            knowledgeType: 'made_up_type',
            scope: 'board',
            sourceType: 'human_note',
            sourceId: null,
            includedAt: '2026-08-03T12:30:00Z',
          },
        ],
      }),
    ).toThrow();
  });

  it('parses compaction status with an active batch', () => {
    const parsed = compactionStatusSchema.parse({
      state: 'running',
      configured: true,
      agent: { id: ID, name: 'Reviewer', enabled: true, connector: 'mock' },
      queuedCount: 0,
      blockedCount: 0,
      oldestQueuedAt: null,
      activeBatch: {
        id: REVISION_ID,
        agentId: ID,
        agentName: 'Reviewer',
        runId: '00000000-0000-4000-8000-000000000008',
        status: 'running',
        trigger: 'manual',
        ticketCount: 3,
        candidateCount: null,
        summary: null,
        errorMessage: null,
        createdAt: '2026-09-28T10:00:00Z',
        startedAt: '2026-09-28T10:00:01Z',
        endedAt: null,
      },
      lastBatch: null,
      nextScheduledAt: null,
      intervalSecs: 1800,
      batchMaxTickets: 10,
    });
    expect(parsed.activeBatch?.ticketCount).toBe(3);
  });
});
