import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  KnowledgeItem,
  SimilarNeighbor,
} from '../../lib/schemas/knowledge';
import { REJECT_PRESETS, guidanceForType } from './curationGuide';
import { KnowledgePage } from './KnowledgePage';

const mocks = vi.hoisted(() => ({
  items: [] as KnowledgeItem[],
  filter: null as unknown,
  similarItems: [] as SimilarNeighbor[],
  similarEnabled: null as boolean | null,
  create: vi.fn(),
  approve: vi.fn(),
  reject: vi.fn(),
  edit: vi.fn(),
  supersede: vi.fn(),
  stale: vi.fn(),
  expire: vi.fn(),
  openTicket: vi.fn(),
  fetchKnowledgeItem: vi.fn(),
}));

const neighbor: SimilarNeighbor = {
  itemId: '00000000-0000-4000-8000-000000000099',
  revisionId: '00000000-0000-4000-8000-000000000098',
  title: 'Existing feedback loop',
  knowledgeType: 'test_command',
  scope: 'board',
  boardId: '00000000-0000-4000-8000-000000000003',
  score: 0.98123,
  status: 'approved',
};

const item: KnowledgeItem = {
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
  lastUsedAt: '2026-08-03T12:30:00Z',
  createdAt: '2026-08-03T11:00:00Z',
  updatedAt: '2026-08-03T12:00:00Z',
};

function mutation(mutateAsync: ReturnType<typeof vi.fn>) {
  return { mutateAsync, isPending: false };
}

vi.mock('../boards/useBoards', () => ({
  useBoards: () => ({
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

vi.mock('./CompactionStatusStrip', () => ({
  CompactionStatusStrip: () => <div data-testid="compaction-strip-stub" />,
}));

vi.mock('./useKnowledge', () => ({
  useKnowledge: (filter: unknown) => {
    mocks.filter = filter;
    return {
      data: { pages: [{ items: mocks.items, nextCursor: null }] },
      isLoading: false,
      isError: false,
      hasNextPage: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
      fetchNextPage: vi.fn(),
    };
  },
  useSimilarKnowledge: (_itemId: string, enabled: boolean) => {
    mocks.similarEnabled = enabled;
    return {
      data: { items: mocks.similarItems },
      isLoading: false,
      isError: false,
      isFetching: false,
    };
  },
  fetchKnowledgeItem: (id: string) => mocks.fetchKnowledgeItem(id),
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
    mocks.items = [item];
    mocks.similarItems = [];
    mocks.similarEnabled = null;
    mocks.create.mockResolvedValue(item);
    mocks.approve.mockResolvedValue({ ...item, status: 'approved' });
    mocks.fetchKnowledgeItem.mockResolvedValue({
      ...item,
      id: neighbor.itemId,
      version: 7,
      status: 'approved',
      title: neighbor.title,
    });
  });

  it('shows governed lifecycle tabs and audit metadata', () => {
    render(<KnowledgePage />);

    expect(screen.getByRole('heading', { name: 'Knowledge' })).toBeVisible();
    expect(screen.getByRole('tab', { name: 'Pending' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByText('Fast feedback loop')).toBeVisible();
    expect(screen.queryByText(/Embedding/)).toBeNull();
    expect(screen.getByTestId('compaction-strip-stub')).toBeInTheDocument();
    expect(screen.getByText(/3 runs/)).toBeVisible();
    expect(screen.getByText(/Supersedes 00000000/)).toBeVisible();
    expect(screen.getByText(/Source run 00000000/)).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(mocks.openTicket).toHaveBeenCalledWith(item.sourceId);
  });

  it('shows compaction provenance with openable source tickets', () => {
    mocks.items = [
      {
        ...item,
        compactionBatchId: '00000000-0000-4000-8000-000000000020',
        compactionAgentId: '00000000-0000-4000-8000-000000000010',
        compactionAgentName: 'Backend Agent',
        sourceTicketIds: [
          '00000000-0000-4000-8000-000000000004',
          'aaaaaaaa-0000-4000-8000-000000000021',
        ],
      },
    ];
    render(<KnowledgePage />);

    const provenance = screen.getByTestId('compaction-provenance');
    expect(provenance).toHaveTextContent('From 2 tickets');
    expect(provenance).toHaveTextContent('Proposed by Backend Agent');
    fireEvent.click(
      within(provenance).getByRole('button', { name: 'Open source ticket aaaaaaaa' }),
    );
    expect(mocks.openTicket).toHaveBeenCalledWith('aaaaaaaa-0000-4000-8000-000000000021');
  });

  it('shows Pending inbox litmus and hides it on other tabs', () => {
    render(<KnowledgePage />);

    const guidance = screen.getByRole('complementary', {
      name: 'Pending inbox guidance',
    });
    expect(guidance).toHaveTextContent(
      'Would a different ticket next month still need this exact rule?',
    );
    expect(guidance).toHaveTextContent(/eligible for retrieval/);
    expect(guidance).toHaveTextContent(/not instruction authority/);

    const typeGuidance = guidanceForType('test_command');
    expect(screen.getByTestId('pending-type-guidance')).toHaveTextContent(
      typeGuidance.approveExample,
    );
    expect(screen.getByTestId('pending-type-guidance')).toHaveTextContent(
      typeGuidance.rejectExample,
    );

    mocks.items = [
      { ...item, status: 'approved', activeRevisionId: item.revisionId },
    ];
    fireEvent.click(screen.getByRole('tab', { name: 'Approved' }));
    expect(
      screen.queryByRole('complementary', { name: 'Pending inbox guidance' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId('pending-type-guidance')).not.toBeInTheDocument();

    mocks.items = [{ ...item, status: 'rejected' }];
    fireEvent.click(screen.getByRole('tab', { name: 'Rejected' }));
    expect(
      screen.queryByRole('complementary', { name: 'Pending inbox guidance' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId('pending-type-guidance')).not.toBeInTheDocument();

    mocks.items = [{ ...item, status: 'stale' }];
    fireEvent.click(screen.getByRole('tab', { name: 'Stale' }));
    expect(
      screen.queryByRole('complementary', { name: 'Pending inbox guidance' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId('pending-type-guidance')).not.toBeInTheDocument();
  });

  it('frames approval as retrieval eligibility in the page header', () => {
    render(<KnowledgePage />);

    expect(screen.getByText('Human-governed · fail-closed')).toBeVisible();
    expect(
      screen.getAllByText(/eligible for retrieval as untrusted reference data/).length,
    ).toBeGreaterThan(0);
  });

  it('uses the selected status and sends optimistic versions for approval', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('tab', { name: 'Rejected' }));
    expect(mocks.filter).toMatchObject({ status: 'rejected' });

    fireEvent.click(screen.getByRole('button', { name: 'Approve anyway' }));
    await waitFor(() => {
      expect(mocks.approve).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
      });
    });
  });

  it('shows No close matches when similar list is empty', () => {
    mocks.similarItems = [];
    render(<KnowledgePage />);

    const assist = screen.getByRole('region', { name: 'Near-duplicate assist' });
    expect(assist).toHaveTextContent('No close matches');
    expect(mocks.similarEnabled).toBe(true);
  });

  it('renders similar neighbors and opens the approved neighbor', async () => {
    mocks.similarItems = [neighbor];
    const approvedNeighbor: KnowledgeItem = {
      ...item,
      id: neighbor.itemId,
      status: 'approved',
      title: neighbor.title,
      activeRevisionId: neighbor.revisionId,
    };
    mocks.items = [item];
    render(<KnowledgePage />);

    const assist = screen.getByRole('region', { name: 'Near-duplicate assist' });
    expect(within(assist).getByText(neighbor.title)).toBeVisible();
    expect(within(assist).getByText(/0.981/)).toBeVisible();

    mocks.items = [approvedNeighbor];
    fireEvent.click(within(assist).getByRole('button', { name: /Open/i }));
    await waitFor(() => {
      expect(mocks.filter).toMatchObject({ status: 'approved' });
    });
    expect(screen.getByText(neighbor.title)).toBeVisible();
  });

  it('rejects as duplicate with cited neighbor and expectedVersion', async () => {
    mocks.similarItems = [neighbor];
    render(<KnowledgePage />);

    fireEvent.click(
      screen.getByRole('button', { name: 'Reject as duplicate' }),
    );
    await waitFor(() => {
      expect(mocks.reject).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        reason: expect.stringMatching(
          /Duplicate of existing approved knowledge.*Existing feedback loop.*00000000/,
        ),
      });
    });
  });

  it('starts supersede against neighbor then rejects pending as duplicate', async () => {
    mocks.similarItems = [neighbor];
    mocks.supersede.mockResolvedValue({
      ...item,
      id: '00000000-0000-4000-8000-000000000088',
      status: 'pending',
    });
    mocks.reject.mockResolvedValue({ ...item, status: 'rejected' });
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Start supersede' }));
    await waitFor(() => {
      expect(mocks.fetchKnowledgeItem).toHaveBeenCalledWith(neighbor.itemId);
    });
    await waitFor(() => {
      expect(mocks.supersede).toHaveBeenCalledWith({
        id: neighbor.itemId,
        expectedVersion: 7,
        replacement: {
          scope: item.scope,
          boardId: item.boardId,
          agentId: item.agentId,
          knowledgeType: item.knowledgeType,
          title: item.title,
          content: item.content,
          sourceType: item.sourceType,
          sourceId: item.sourceId,
          sourceRunId: item.sourceRunId,
          confidence: item.confidence,
        },
      });
    });
    await waitFor(() => {
      expect(mocks.reject).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        reason: expect.stringMatching(
          /Duplicate of existing approved knowledge.*Existing feedback loop.*00000000/,
        ),
      });
    });
  });

  it('approves anyway with pending expectedVersion', async () => {
    mocks.similarItems = [neighbor];
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Approve anyway' }));
    await waitFor(() => {
      expect(mocks.approve).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
      });
    });
  });

  it('creates a typed and board-scoped manual candidate', async () => {
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
    fireEvent.change(controls.getByLabelText('Board'), {
      target: { value: '00000000-0000-4000-8000-000000000003' },
    });
    fireEvent.click(controls.getByRole('button', { name: 'Add to Pending' }));

    await waitFor(() => {
      expect(mocks.create).toHaveBeenCalledWith({
        scope: 'board',
        boardId: '00000000-0000-4000-8000-000000000003',
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

  it('sends optimistic versions for edit and rejection actions', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    fireEvent.change(screen.getByLabelText('Revision title'), {
      target: { value: 'Updated feedback loop' },
    });
    fireEvent.change(screen.getByLabelText('Revision content'), {
      target: { value: 'Run the focused tests before review.' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save revision' }));
    await waitFor(() => {
      expect(mocks.edit).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        patch: {
          title: 'Updated feedback loop',
          content: 'Run the focused tests before review.',
          confidence: 'high',
        },
      });
    });

    fireEvent.click(screen.getByRole('button', { name: 'Reject' }));
    fireEvent.change(screen.getByLabelText('Reason (optional)'), {
      target: { value: 'Too specific to this incident.' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reject candidate' }));
    await waitFor(() => {
      expect(mocks.reject).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        reason: 'Too specific to this incident.',
      });
    });
  });

  it('populates reject reason from presets and submits clear prose', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Reject' }));
    const presets = screen.getByRole('group', { name: 'Reject reason presets' });
    const oneOff = REJECT_PRESETS.find((preset) => preset.id === 'one_off')!;
    fireEvent.click(within(presets).getByRole('button', { name: oneOff.label }));

    expect(screen.getByLabelText('Reason (optional)')).toHaveValue(oneOff.reasonText);

    fireEvent.click(screen.getByRole('button', { name: 'Reject candidate' }));
    await waitFor(() => {
      expect(mocks.reject).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        reason: oneOff.reasonText,
      });
    });
    expect(mocks.reject.mock.calls[0][0].reason).not.toBe('one_off');
  });

  it('allows empty reject reason without a preset', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Reject' }));
    fireEvent.click(screen.getByRole('button', { name: 'Reject candidate' }));
    await waitFor(() => {
      expect(mocks.reject).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
        reason: null,
      });
    });
  });

  it('exposes supersede, stale, and expire actions for approved knowledge', async () => {
    const approved = {
      ...item,
      status: 'approved' as const,
      activeRevisionId: item.revisionId,
      expiresAt: null,
    };
    mocks.items = [approved];
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Supersede' }));
    fireEvent.change(screen.getByLabelText('Revision title'), {
      target: { value: 'Replacement feedback loop' },
    });
    fireEvent.change(screen.getByLabelText('Revision content'), {
      target: { value: 'Use the replacement command.' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create replacement' }));
    await waitFor(() => {
      expect(mocks.supersede).toHaveBeenCalledWith({
        id: approved.id,
        expectedVersion: approved.version,
        replacement: {
          scope: approved.scope,
          boardId: approved.boardId,
          agentId: approved.agentId,
          knowledgeType: approved.knowledgeType,
          title: 'Replacement feedback loop',
          content: 'Use the replacement command.',
          sourceType: 'human_note',
          sourceId: null,
          sourceRunId: null,
          confidence: approved.confidence,
        },
      });
    });

    fireEvent.click(screen.getByRole('button', { name: 'Mark stale' }));
    await waitFor(() => {
      expect(mocks.stale).toHaveBeenCalledWith({
        id: approved.id,
        expectedVersion: approved.version,
      });
    });

    fireEvent.click(screen.getByRole('button', { name: 'Expire now' }));
    await waitFor(() => {
      expect(mocks.expire).toHaveBeenCalledWith({
        id: approved.id,
        expectedVersion: approved.version,
      });
    });
  });
});
