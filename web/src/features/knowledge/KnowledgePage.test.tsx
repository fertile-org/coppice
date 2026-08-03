import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { KnowledgeItem } from '../../lib/schemas/knowledge';
import { KnowledgePage } from './KnowledgePage';

const mocks = vi.hoisted(() => ({
  filter: null as unknown,
  create: vi.fn(),
  approve: vi.fn(),
  reject: vi.fn(),
  edit: vi.fn(),
  supersede: vi.fn(),
  stale: vi.fn(),
  expire: vi.fn(),
  openTicket: vi.fn(),
}));

const item: KnowledgeItem = {
  id: '00000000-0000-4000-8000-000000000001',
  version: 2,
  status: 'pending',
  revisionId: '00000000-0000-4000-8000-000000000002',
  revisionNumber: 1,
  activeRevisionId: null,
  scope: 'project',
  projectId: '00000000-0000-4000-8000-000000000003',
  projectName: 'Coppice',
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
  embeddingStatus: 'not_requested',
  embeddingError: null,
  usageCount: 3,
  lastUsedAt: '2026-08-03T12:30:00Z',
  createdAt: '2026-08-03T11:00:00Z',
  updatedAt: '2026-08-03T12:00:00Z',
};

function mutation(mutateAsync: ReturnType<typeof vi.fn>) {
  return { mutateAsync, isPending: false };
}

vi.mock('../projects/useProjects', () => ({
  useProjects: () => ({
    data: [
      {
        id: '00000000-0000-4000-8000-000000000003',
        name: 'Coppice',
        slug: 'coppice',
        createdAt: '2026-08-03T00:00:00Z',
      },
    ],
  }),
}));

vi.mock('../agents/useAgents', () => ({
  useAgents: () => ({
    data: [
      {
        id: '00000000-0000-4000-8000-000000000010',
        name: 'Backend Agent',
        enabled: true,
      },
    ],
  }),
}));

vi.mock('../auth/useSession', () => ({
  useSession: () => ({ user: { role: 'admin' } }),
}));

vi.mock('../tickets/useOpenTicket', () => ({
  useOpenTicket: () => mocks.openTicket,
}));

vi.mock('./useKnowledge', () => ({
  useKnowledge: (filter: unknown) => {
    mocks.filter = filter;
    return {
      data: { pages: [{ items: [item], nextCursor: null }] },
      isLoading: false,
      isError: false,
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
      fetchNextPage: vi.fn(),
    };
  },
  useCreateKnowledge: () => mutation(mocks.create),
  useApproveKnowledge: () => mutation(mocks.approve),
  useRejectKnowledge: () => mutation(mocks.reject),
  useEditKnowledge: () => mutation(mocks.edit),
  useSupersedeKnowledge: () => mutation(mocks.supersede),
  useMarkKnowledgeStale: () => mutation(mocks.stale),
  useExpireKnowledge: () => mutation(mocks.expire),
}));

describe('KnowledgePage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.create.mockResolvedValue(item);
    mocks.approve.mockResolvedValue({ ...item, status: 'approved' });
  });

  it('shows governed lifecycle tabs and audit metadata', () => {
    render(<KnowledgePage />);

    expect(screen.getByRole('heading', { name: 'Knowledge' })).toBeVisible();
    expect(screen.getByRole('tab', { name: 'Pending' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByText('Fast feedback loop')).toBeVisible();
    expect(screen.getByText('Embedding · Not Requested')).toBeVisible();
    expect(screen.getByText(/3 runs/)).toBeVisible();
    expect(screen.getByText(/Supersedes 00000000/)).toBeVisible();
    expect(screen.getByText(/Source run 00000000/)).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(mocks.openTicket).toHaveBeenCalledWith(item.sourceId);
  });

  it('uses the selected status and sends optimistic versions for approval', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('tab', { name: 'Rejected' }));
    expect(mocks.filter).toMatchObject({ status: 'rejected' });

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => {
      expect(mocks.approve).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
      });
    });
  });

  it('creates a typed and project-scoped manual candidate', async () => {
    render(<KnowledgePage />);
    const form = screen
      .getByRole('heading', { name: 'Manual candidate' })
      .closest('form');
    expect(form).not.toBeNull();
    const controls = within(form!);

    fireEvent.change(controls.getByLabelText('Title'), {
      target: { value: 'Use the fast test target' },
    });
    fireEvent.change(controls.getByLabelText('Knowledge'), {
      target: { value: 'Run make test-unit before the full suite.' },
    });
    fireEvent.change(controls.getByLabelText('Type'), {
      target: { value: 'test_command' },
    });
    fireEvent.change(controls.getByLabelText('Project'), {
      target: { value: '00000000-0000-4000-8000-000000000003' },
    });
    fireEvent.click(controls.getByRole('button', { name: 'Add to Pending' }));

    await waitFor(() => {
      expect(mocks.create).toHaveBeenCalledWith({
        scope: 'project',
        projectId: '00000000-0000-4000-8000-000000000003',
        agentId: null,
        knowledgeType: 'test_command',
        title: 'Use the fast test target',
        content: 'Run make test-unit before the full suite.',
        sourceType: 'human_note',
        sourceId: null,
        sourceRunId: null,
        confidence: 'medium',
      });
    });
  });
});
