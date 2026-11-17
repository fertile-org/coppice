import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { selectComboboxOption } from '../../test/combobox';
import {
  item,
  mutation,
  neighbor,
  resetKnowledgePageMocks,
} from './KnowledgePage.test.shared';
import { KnowledgePage } from './KnowledgePage';

const mocks = vi.hoisted(() => ({
  items: [] as typeof item[],
  filter: null as unknown,
  similarItems: [] as typeof neighbor[],
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
  role: 'admin' as 'admin' | 'member',
  hasNextPage: false,
  toast: { success: vi.fn(), error: vi.fn() },
}));

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
  useSession: () => ({ user: { role: mocks.role } }),
}));

vi.mock('../../components/ToastProvider', () => ({
  useToast: () => mocks.toast,
}));

vi.mock('../tickets/useOpenTicket', () => ({
  useOpenTicket: () => mocks.openTicket,
}));

vi.mock('./CompactionStatusStrip', () => ({
  CompactionStatusStrip: () => <div data-testid="compaction-strip-stub" />,
}));

// Dialogs live in KnowledgePage.dialogs.test.tsx — stub here to drop Radix/cmdk RSS.
vi.mock('./KnowledgeDialogs', () => ({
  KnowledgeFormDialog: () => null,
  RejectKnowledgeDialog: () => null,
}));

vi.mock('./useKnowledge', () => ({
  useKnowledge: (filter: unknown) => {
    mocks.filter = filter;
    return {
      data: { pages: [{ items: mocks.items, nextCursor: null }] },
      isLoading: false,
      isError: false,
      hasNextPage: mocks.hasNextPage,
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
  useKnowledgeFlags: () => ({
    data: {
      items: [
        {
          verdict: 'wrong',
          reason: 'The command moved.',
          agentName: 'Backend',
          createdAt: '2026-10-30T12:00:00Z',
        },
      ],
    },
    isLoading: false,
    isError: false,
  }),
  useDismissKnowledgeFlags: () => mutation(vi.fn()),
  useCreateKnowledge: () => mutation(mocks.create),
  useApproveKnowledge: () => mutation(mocks.approve),
  useRejectKnowledge: () => mutation(mocks.reject),
  useEditKnowledge: () => mutation(mocks.edit),
  useSupersedeKnowledge: () => mutation(mocks.supersede),
  useMarkKnowledgeStale: () => mutation(mocks.stale),
  useExpireKnowledge: () => mutation(mocks.expire),
}));

describe('KnowledgePage', () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    resetKnowledgePageMocks(mocks);
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

  it('lists approved items with open flags on the Flagged tab', () => {
    mocks.items = [{ ...item, status: 'approved', openFlagCount: 1 }];
    render(<KnowledgePage />);
    expect(mocks.filter).toEqual({ status: 'pending' });

    fireEvent.click(screen.getByRole('tab', { name: 'Flagged' }));

    expect(screen.getByRole('tab', { name: 'Flagged' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(mocks.filter).toEqual({ status: 'approved', flagged: true });
    expect(screen.getByText('Flagged by agents (1)')).toBeVisible();
    expect(screen.getByText('The command moved.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Dismiss flags' })).toBeVisible();

    mocks.items = [];
    fireEvent.click(screen.getByRole('tab', { name: 'Pending' }));
    fireEvent.click(screen.getByRole('tab', { name: 'Flagged' }));
    expect(screen.getByRole('heading', { name: 'No flagged knowledge' })).toBeVisible();
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

  it('drops agent-facing guidance and the fail-closed badge', () => {
    render(<KnowledgePage />);

    expect(
      screen.queryByRole('complementary', { name: 'Pending inbox guidance' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId('pending-type-guidance')).not.toBeInTheDocument();
    expect(screen.queryByText('Human-governed · fail-closed')).not.toBeInTheDocument();
    expect(screen.queryByText(/Would a different ticket/)).not.toBeInTheDocument();
  });

  it('shows a main Approve button on pending cards that approves with expectedVersion', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => {
      expect(mocks.approve).toHaveBeenCalledWith({
        id: item.id,
        expectedVersion: item.version,
      });
    });
  });

  it('hides Approve anyway when there are no near-duplicates', () => {
    mocks.similarItems = [];
    render(<KnowledgePage />);

    expect(screen.getByRole('button', { name: 'Approve' })).toBeVisible();
    expect(
      screen.queryByRole('button', { name: 'Approve anyway' }),
    ).not.toBeInTheDocument();
  });

  it('shows Approve anyway when there is a near-duplicate', () => {
    mocks.similarItems = [neighbor];
    render(<KnowledgePage />);

    expect(
      within(screen.getByRole('region', { name: 'Near-duplicate assist' })).getByRole(
        'button',
        { name: 'Approve anyway' },
      ),
    ).toBeVisible();
  });

  it('hides the raw policy reason on pending cards but keeps rejection reasons', () => {
    render(<KnowledgePage />);

    expect(screen.queryByText(/Policy:/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Manual candidates require review/)).not.toBeInTheDocument();
  });

  it('keeps the rejection reason on rejected cards', () => {
    mocks.items = [
      {
        ...item,
        status: 'rejected',
        policyReason: 'Manual candidates require review.',
        rejectionReason: 'One-off for this ticket; not reusable next month.',
      },
    ];
    render(<KnowledgePage />);

    expect(screen.getByText('Rejected:').parentElement).toHaveTextContent(
      'One-off for this ticket; not reusable next month.',
    );
    expect(screen.getByText('Policy:')).toBeVisible();
  });

  it('uses the selected status and sends optimistic versions for approval', async () => {
    mocks.similarItems = [neighbor];
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
    const approvedNeighbor = {
      ...item,
      id: neighbor.itemId,
      status: 'approved' as const,
      title: neighbor.title,
      activeRevisionId: neighbor.revisionId,
    };
    mocks.items = [item];
    render(<KnowledgePage />);

    const assist = screen.getByRole('region', { name: 'Near-duplicate assist' });
    expect(within(assist).getByText(neighbor.title)).toBeVisible();
    expect(within(assist).getByText(/match 98%/)).toBeVisible();

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

    fireEvent.click(screen.getByRole('button', { name: 'Reject as duplicate' }));
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
        citationsFromItemId: item.id,
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

  it('filters by board and type comboboxes and searches loaded items', () => {
    mocks.items = [
      item,
      {
        ...item,
        id: '00000000-0000-4000-8000-000000000031',
        title: 'Security header rule',
        content: 'Always set CSP.',
        knowledgeType: 'security_rule',
      },
    ];
    render(<KnowledgePage />);

    const board = screen.getByRole('combobox', { name: 'Filter by board' });
    selectComboboxOption(board, 'Coppice');
    expect(board).toHaveTextContent('Coppice');
    expect(mocks.filter).toMatchObject({ boardId: '00000000-0000-4000-8000-000000000003' });

    const type = screen.getByRole('combobox', { name: 'Filter by type' });
    selectComboboxOption(type, 'Test command');
    expect(type).toHaveTextContent('Test command');
    expect(mocks.filter).toMatchObject({ knowledgeType: 'test_command' });

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search knowledge' }), {
      target: { value: 'csp' },
    });
    expect(screen.getByText('Security header rule')).toBeVisible();
    expect(screen.queryByText('Fast feedback loop')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(screen.getByText('Fast feedback loop')).toBeVisible();
    expect(board).toHaveTextContent('All scopes');
    expect(mocks.filter).toMatchObject({ boardId: undefined, knowledgeType: undefined });
  });

  it('shows empty states with an Add knowledge call to action', () => {
    mocks.items = [];
    render(<KnowledgePage />);

    expect(screen.getByRole('heading', { name: 'No pending knowledge' })).toBeVisible();
    expect(screen.getAllByRole('button', { name: 'Add knowledge' })).toHaveLength(2);

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search knowledge' }), {
      target: { value: 'nothing' },
    });
    expect(screen.getByRole('heading', { name: 'No matching knowledge' })).toBeVisible();
    fireEvent.click(screen.getAllByRole('button', { name: 'Clear filters' })[1]);
    expect(screen.getByRole('heading', { name: 'No pending knowledge' })).toBeVisible();
  });

  it('is read-only for non-admins', () => {
    mocks.role = 'member';
    render(<KnowledgePage />);

    expect(screen.queryByRole('button', { name: 'Add knowledge' })).not.toBeInTheDocument();
    expect(screen.getByText(/Read-only/)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Approve' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reject' })).not.toBeInTheDocument();
  });

  it('exposes stale and expire actions for approved knowledge', async () => {
    const approved = {
      ...item,
      status: 'approved' as const,
      activeRevisionId: item.revisionId,
      expiresAt: null,
    };
    mocks.items = [approved];
    render(<KnowledgePage />);

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
