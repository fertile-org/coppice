import { describe, expect, it } from 'vitest';
import {
  knowledgeItemSchema,
  knowledgeUsageListSchema,
} from './knowledge';

const ID = '00000000-0000-4000-8000-000000000001';
const REVISION_ID = '00000000-0000-4000-8000-000000000002';

describe('knowledge schemas', () => {
  it('parses lifecycle, provenance, embedding, and usage metadata', () => {
    const parsed = knowledgeItemSchema.parse({
      id: ID,
      version: 3,
      status: 'approved',
      revisionId: REVISION_ID,
      revisionNumber: 2,
      activeRevisionId: REVISION_ID,
      scope: 'project',
      projectId: '00000000-0000-4000-8000-000000000003',
      projectName: 'Coppice',
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
      embeddingStatus: 'ready',
      embeddingError: null,
      usageCount: 4,
      lastUsedAt: '2026-08-03T12:30:00Z',
      createdAt: '2026-08-03T11:00:00Z',
      updatedAt: '2026-08-03T12:00:00Z',
    });

    expect(parsed.revisionId).toBe(REVISION_ID);
    expect(parsed.usageCount).toBe(4);
  });

  it('rejects unknown types and preserves the exact used revision', () => {
    expect(() =>
      knowledgeUsageListSchema.parse({
        items: [
          {
            itemId: ID,
            revisionId: REVISION_ID,
            rank: 1,
            similarity: 0.91,
            tokenCount: 12,
            renderedContent: '<knowledge>exact revision</knowledge>',
            title: 'Fast tests',
            knowledgeType: 'made_up_type',
            scope: 'project',
            sourceType: 'human_note',
            sourceId: null,
            includedAt: '2026-08-03T12:30:00Z',
          },
        ],
      }),
    ).toThrow();
  });
});
