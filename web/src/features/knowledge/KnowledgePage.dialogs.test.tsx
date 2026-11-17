import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { selectComboboxOption } from '../../test/combobox';
import { REJECT_PRESETS } from './curationGuide';
import {
  item,
  mutation,
  resetKnowledgePageMocks,
} from './KnowledgePage.test.shared';
import { KnowledgePage } from './KnowledgePage';

const mocks = vi.hoisted(() => ({
  items: [] as typeof item[],
  filter: null as unknown,
  similarItems: [] as unknown[],
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
    data: { items: [] },
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

describe('KnowledgePage dialogs', () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    resetKnowledgePageMocks(mocks);
  });

  it('creates a typed and board-scoped candidate from the Add knowledge modal', async () => {
    render(<KnowledgePage />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Add knowledge' }));
    const dialog = screen.getByRole('dialog', { name: 'Add knowledge' });
    const controls = within(dialog);

    fireEvent.change(controls.getByLabelText('Title'), {
      target: { value: 'Use the fast test target' },
    });
    fireEvent.change(controls.getByLabelText('Knowledge'), {
      target: { value: 'Run make test-unit before the full suite.' },
    });
    selectComboboxOption(controls.getByLabelText('Type'), 'Test command');
    expect(controls.getByLabelText('Type')).toHaveTextContent('Test command');
    selectComboboxOption(controls.getByLabelText('Board'), 'Coppice');
    expect(controls.getByLabelText('Board')).toHaveTextContent('Coppice');
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
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
    expect(mocks.toast.success).toHaveBeenCalledWith('Knowledge added to Pending.');
  });

  it('creates agent-scoped knowledge with scope, agent, and confidence comboboxes', async () => {
    render(<KnowledgePage />);
    fireEvent.click(screen.getByRole('button', { name: 'Add knowledge' }));
    const controls = within(screen.getByRole('dialog'));

    fireEvent.change(controls.getByLabelText('Title'), { target: { value: 'Agent rule' } });
    fireEvent.change(controls.getByLabelText('Knowledge'), {
      target: { value: 'Backend agent runs cargo clippy.' },
    });
    selectComboboxOption(controls.getByLabelText('Scope'), /Board \+ agent/);
    selectComboboxOption(controls.getByLabelText('Board'), 'Coppice');
    selectComboboxOption(controls.getByLabelText('Agent'), 'Backend Agent');
    selectComboboxOption(controls.getByLabelText('Confidence'), 'High');
    fireEvent.click(controls.getByRole('button', { name: 'Add to Pending' }));

    await waitFor(() => {
      expect(mocks.create).toHaveBeenCalledWith(
        expect.objectContaining({
          scope: 'agent',
          boardId: '00000000-0000-4000-8000-000000000003',
          agentId: '00000000-0000-4000-8000-000000000010',
          confidence: 'high',
        }),
      );
    });
  });

  it('keeps validation and server errors inside the modal', async () => {
    mocks.create.mockRejectedValueOnce(new Error('boom'));
    render(<KnowledgePage />);
    fireEvent.click(screen.getByRole('button', { name: 'Add knowledge' }));
    const controls = within(screen.getByRole('dialog'));

    fireEvent.change(controls.getByLabelText('Title'), { target: { value: 'Rule' } });
    fireEvent.change(controls.getByLabelText('Knowledge'), { target: { value: 'Body' } });
    fireEvent.click(controls.getByRole('button', { name: 'Add to Pending' }));
    expect(controls.getByRole('alert')).toHaveTextContent('Choose a board for this scope.');
    expect(mocks.create).not.toHaveBeenCalled();

    selectComboboxOption(controls.getByLabelText('Board'), 'Coppice');
    fireEvent.click(controls.getByRole('button', { name: 'Add to Pending' }));
    await waitFor(() => {
      expect(controls.getByRole('alert')).toHaveTextContent('Unable to create candidate.');
    });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(mocks.toast.success).not.toHaveBeenCalled();
  });

  it('resets the create form each time the modal opens', async () => {
    render(<KnowledgePage />);
    fireEvent.click(screen.getByRole('button', { name: 'Add knowledge' }));
    fireEvent.change(within(screen.getByRole('dialog')).getByLabelText('Title'), {
      target: { value: 'Draft' },
    });
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Add knowledge' }));
    expect(within(screen.getByRole('dialog')).getByLabelText('Title')).toHaveValue('');
  });

  it('opens Add knowledge from the empty state', () => {
    mocks.items = [];
    render(<KnowledgePage />);
    fireEvent.click(screen.getAllByRole('button', { name: 'Add knowledge' })[1]);
    expect(screen.getByRole('dialog', { name: 'Add knowledge' })).toBeInTheDocument();
  });

  it('sends optimistic versions for edit and rejection actions', async () => {
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    const editor = within(screen.getByRole('dialog', { name: 'Create a new revision' }));
    expect(editor.getByLabelText('Title')).toHaveValue(item.title);
    expect(editor.getByLabelText('Confidence')).toHaveTextContent('High');
    expect(editor.queryByLabelText('Scope')).not.toBeInTheDocument();
    fireEvent.change(editor.getByLabelText('Title'), {
      target: { value: 'Updated feedback loop' },
    });
    fireEvent.change(editor.getByLabelText('Knowledge'), {
      target: { value: 'Run the focused tests before review.' },
    });
    fireEvent.click(editor.getByRole('button', { name: 'Save revision' }));
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
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
    expect(mocks.toast.success).toHaveBeenCalledWith('Revision saved.');

    fireEvent.click(screen.getByRole('button', { name: 'Reject' }));
    expect(screen.getByRole('dialog', { name: 'Reject candidate' })).toBeInTheDocument();
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
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
    expect(mocks.toast.success).toHaveBeenCalledWith('Candidate rejected.');
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

  it('creates a supersede replacement from the approved card action', async () => {
    const approved = {
      ...item,
      status: 'approved' as const,
      activeRevisionId: item.revisionId,
      expiresAt: null,
    };
    mocks.items = [approved];
    render(<KnowledgePage />);

    fireEvent.click(screen.getByRole('button', { name: 'Supersede' }));
    const editor = within(
      screen.getByRole('dialog', { name: 'Create replacement candidate' }),
    );
    fireEvent.change(editor.getByLabelText('Title'), {
      target: { value: 'Replacement feedback loop' },
    });
    fireEvent.change(editor.getByLabelText('Knowledge'), {
      target: { value: 'Use the replacement command.' },
    });
    fireEvent.click(editor.getByRole('button', { name: 'Create replacement' }));
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
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });
});
