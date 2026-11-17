import { vi } from 'vitest';
import type { KnowledgeItem, SimilarNeighbor } from '../../lib/schemas/knowledge';

export type KnowledgePageMocks = {
  items: KnowledgeItem[];
  filter: unknown;
  similarItems: SimilarNeighbor[];
  similarEnabled: boolean | null;
  create: ReturnType<typeof vi.fn>;
  approve: ReturnType<typeof vi.fn>;
  reject: ReturnType<typeof vi.fn>;
  edit: ReturnType<typeof vi.fn>;
  supersede: ReturnType<typeof vi.fn>;
  stale: ReturnType<typeof vi.fn>;
  expire: ReturnType<typeof vi.fn>;
  openTicket: ReturnType<typeof vi.fn>;
  fetchKnowledgeItem: ReturnType<typeof vi.fn>;
  role: 'admin' | 'member';
  hasNextPage: boolean;
  toast: { success: ReturnType<typeof vi.fn>; error: ReturnType<typeof vi.fn> };
};

export const neighbor: SimilarNeighbor = {
  itemId: '00000000-0000-4000-8000-000000000099',
  revisionId: '00000000-0000-4000-8000-000000000098',
  title: 'Existing feedback loop',
  knowledgeType: 'test_command',
  scope: 'board',
  boardId: '00000000-0000-4000-8000-000000000003',
  score: 0.98123,
  status: 'approved',
};

export const item: KnowledgeItem = {
  id: '00000000-0000-4000-8000-000000000001',
  version: 2,
  status: 'pending',
  revisionId: '00000000-0000-4000-8000-000000000002',
  revisionNumber: 1,
  activeRevisionId: null,
  scope: 'board',
  boardId: '00000000-0000-4000-8000-000000000003',
  boardName: 'Coppice',
  agentId: null,
  agentName: null,
  knowledgeType: 'test_command',
  title: 'Fast feedback loop',
  content: 'Run make test-unit while iterating.',
  sourceType: 'ticket',
  sourceId: '00000000-0000-4000-8000-000000000004',
  sourceRunId: '00000000-0000-4000-8000-000000000005',
  confidence: 'high',
  approvedBy: null,
  approvedAt: null,
  approvalMode: null,
  policyDecision: 'pending_human_review',
  policyReason: 'Manual candidates require review.',
  rejectionReason: null,
  expiresAt: '2030-08-03T12:00:00Z',
  supersedesItemId: '00000000-0000-4000-8000-000000000006',
  supersededBy: null,
  staleAt: null,
  compactionBatchId: null,
  compactionAgentId: null,
  compactionAgentName: null,
  sourceTicketIds: [],
  usageCount: 3,
  openFlagCount: 0,
  lastUsedAt: '2026-08-03T12:30:00Z',
  createdAt: '2026-08-03T11:00:00Z',
  updatedAt: '2026-08-03T12:00:00Z',
};

export function mutation(mutateAsync: ReturnType<typeof vi.fn>) {
  return { mutateAsync, isPending: false };
}

export function resetKnowledgePageMocks(mocks: KnowledgePageMocks) {
  vi.clearAllMocks();
  mocks.items = [item];
  mocks.similarItems = [];
  mocks.similarEnabled = null;
  mocks.role = 'admin';
  mocks.hasNextPage = false;
  mocks.create.mockResolvedValue(item);
  mocks.approve.mockResolvedValue({ ...item, status: 'approved' });
  mocks.fetchKnowledgeItem.mockResolvedValue({
    ...item,
    id: neighbor.itemId,
    version: 7,
    status: 'approved',
    title: neighbor.title,
  });
}
