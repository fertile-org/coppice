import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ToastProvider } from '../../components/ToastProvider';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { Ticket } from '../board/useTickets';
import { GitStatusRow } from './GitStatusRow';
import { gitStateCopy, type TicketGitState } from './gitStateCopy';
import type { TicketGitInfo } from './useTicket';

const pushMainMutateAsync = vi.fn();
const invalidated: unknown[][] = [];
const gitInfoState: { data: TicketGitInfo | undefined } = { data: undefined };
const gitInfoEnabled: boolean[] = [];
const sessionState: { role: string } = { role: 'admin' };
const reposState: { data: unknown[] | undefined } = { data: undefined };

vi.mock('../auth/useSession', () => ({
  useSession: () => ({
    user: { id: 'u1', email: 'u@example.com', role: sessionState.role },
    loading: false,
    desktopMode: false,
    establishSession: vi.fn(),
    logout: vi.fn(),
  }),
}));

vi.mock('./useTicket', () => ({
  ticketQueryKey: (id: string) => ['ticket', id],
  gitInfoQueryKey: (id: string) => ['ticket-git-info', id],
  useTicketGitInfo: (_id: string, enabled: boolean) => {
    gitInfoEnabled.push(enabled);
    return { data: enabled ? gitInfoState.data : undefined, isLoading: false };
  },
  useMergeTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRebaseTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useResolveConflict: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRemoveWorktree: () => ({ mutateAsync: vi.fn(), isPending: false }),
  usePushTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useCreateTicketPr: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('../repos/useRepos', () => ({
  useRepos: () => ({ data: reposState.data }),
  usePushDefaultBranch: () => ({ mutateAsync: pushMainMutateAsync, isPending: false }),
}));

const gitInfo: TicketGitInfo = {
  ticketBranch: 'agent/TICKET-abc',
  worktreePath: '/tmp/worktrees/TICKET-abc',
  worktreeExists: true,
  defaultBranch: 'main',
  branches: ['main'],
  canPush: true,
};

function makeTicket(state: TicketGitState, overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: '00000000-0000-0000-0000-000000000001',
    boardId: '00000000-0000-0000-0000-000000000002',
    repoId: '00000000-0000-0000-0000-000000000003',
    title: 'Test ticket',
    description: '',
    status: 'done',
    createdBy: 'user',
    createdAt: '2026-06-08T00:00:00.000Z',
    updatedAt: '2026-06-08T00:00:00.000Z',
    lastActivityAt: '2026-06-08T00:00:00.000Z',
    humanReview: {
      headSha: 'abc1234deadbeef',
      shortSha: 'abc1234',
      stale: false,
      gitState: state,
      gitStateReason: null,
      acceptedByCoppice: false,
      mergedSha: state === 'merged' ? 'a1b2c3d4e5f6' : null,
      commentIds: [],
      runIds: [],
    },
    ...overrides,
  };
}

function renderRow(ticket: Ticket, variant: 'panel' | 'card' = 'panel') {
  const client = new QueryClient();
  vi.spyOn(client, 'invalidateQueries').mockImplementation((filters) => {
    invalidated.push([...(filters?.queryKey ?? [])]);
    return Promise.resolve();
  });
  return render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <ConfirmProvider>
          <GitStatusRow ticket={ticket} variant={variant} />
        </ConfirmProvider>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

const withButton: TicketGitState[] = [
  'in_review',
  'not_merged',
  'conflict',
  'main_dirty',
  'not_pushed',
  'worktree_kept',
];

describe('GitStatusRow', () => {
  beforeEach(() => {
    pushMainMutateAsync.mockReset();
    invalidated.length = 0;
    gitInfoState.data = { ...gitInfo };
    gitInfoEnabled.length = 0;
    sessionState.role = 'admin';
    reposState.data = undefined;
  });

  it.each(withButton)('renders the pill and main action for %s', (state) => {
    renderRow(makeTicket(state));
    expect(screen.getByText(gitStateCopy[state].label)).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: gitStateCopy[state].primary! }),
    ).toBeInTheDocument();
  });

  it('shows no main action when the branch changed after review', () => {
    renderRow(makeTicket('changed_after_review'));
    expect(screen.getByText('Changed after review')).toBeInTheDocument();
    for (const name of ['Rebase…', 'Merge', 'Ask agent', 'Retry merge', 'Push main']) {
      expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    }
  });

  it('keeps Merge out of the dropdown while in review', () => {
    renderRow(makeTicket('in_review'));
    fireEvent.click(screen.getByRole('button', { name: 'More git actions' }));
    expect(screen.getByRole('button', { name: 'Push branch' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Merge/ })).not.toBeInTheDocument();
  });

  it('offers Merge into… in the dropdown once accepted', () => {
    renderRow(makeTicket('not_merged'));
    fireEvent.click(screen.getByRole('button', { name: 'More git actions' }));
    expect(screen.getByRole('button', { name: 'Merge into…' })).toBeInTheDocument();
  });

  it('shows the short SHA and no button once merged', () => {
    renderRow(makeTicket('merged'));
    expect(screen.getByText('Merged')).toBeInTheDocument();
    expect(screen.getByText('a1b2c3d')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'More git actions' })).not.toBeInTheDocument();
  });

  it('pushes the default branch from Push main', () => {
    pushMainMutateAsync.mockResolvedValue({ message: 'Pushed main' });
    renderRow(makeTicket('not_pushed'));
    fireEvent.click(screen.getByRole('button', { name: 'Push main' }));
    expect(pushMainMutateAsync).toHaveBeenCalledTimes(1);
  });

  it('refreshes the ticket, git info and board after Push main', async () => {
    pushMainMutateAsync.mockResolvedValue({ message: 'Pushed main' });
    const ticket = makeTicket('not_pushed');
    renderRow(ticket);
    fireEvent.click(screen.getByRole('button', { name: 'Push main' }));
    await waitFor(() => expect(invalidated).toHaveLength(3));
    expect(invalidated).toEqual(
      expect.arrayContaining([
        ['ticket', ticket.id],
        ['ticket-git-info', ticket.id],
        ['tickets'],
      ]),
    );
  });

  it('opens the menu from the keyboard, reaches an item, and Escape returns focus', async () => {
    renderRow(makeTicket('not_merged'));
    const trigger = screen.getByRole('button', { name: 'More git actions' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    fireEvent.click(trigger);
    const group = await screen.findByRole('group', { name: 'Git actions' });
    const item = within(group).getByRole('button', { name: 'Rebase…' });
    item.focus();
    expect(item).toHaveFocus();
    fireEvent.keyDown(item, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Git actions' })).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it('disables the menu trigger when the ticket is archived', () => {
    renderRow(makeTicket('not_merged', { archivedAt: '2026-06-09T00:00:00.000Z' }));
    expect(screen.getByRole('button', { name: 'More git actions' })).toBeDisabled();
  });

  it('hints how to load a conflict when none is known', () => {
    renderRow(makeTicket('conflict'));
    expect(screen.getByText('Retry the merge to load the conflicting files.')).toBeInTheDocument();
  });

  it('prefers the fresher git-info state in the panel and the ticket state on the card', () => {
    gitInfoState.data = { ...gitInfo, gitState: 'conflict' };
    const view = renderRow(makeTicket('not_merged'));
    expect(screen.getByText('Conflict')).toBeInTheDocument();
    view.unmount();
    renderRow(makeTicket('not_merged'), 'card');
    expect(screen.getByText('Not merged')).toBeInTheDocument();
  });

  it('renders the reason line in the panel only', () => {
    gitInfoState.data = { ...gitInfo, gitState: 'main_dirty', gitStateReason: 'Main has uncommitted changes' };
    const view = renderRow(makeTicket('main_dirty'));
    expect(screen.getByTestId('git-state-reason')).toHaveTextContent(
      'Main has uncommitted changes',
    );
    view.unmount();
    renderRow(
      makeTicket('main_dirty', {
        humanReview: {
          ...makeTicket('main_dirty').humanReview!,
          gitStateReason: 'Main has uncommitted changes',
        },
      }),
      'card',
    );
    expect(screen.queryByTestId('git-state-reason')).not.toBeInTheDocument();
    expect(screen.queryByText('Main has uncommitted changes')).not.toBeInTheDocument();
  });

  it('shows the worktree path once, behind the collapsed branch detail', () => {
    renderRow(makeTicket('not_merged'));
    expect(screen.queryByText(gitInfo.worktreePath)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: gitInfo.ticketBranch }));
    expect(screen.getAllByText(gitInfo.worktreePath)).toHaveLength(1);
  });

  it('treats a missing human review as in review before Done', () => {
    gitInfoState.data = { ...gitInfo, gitState: undefined };
    renderRow(makeTicket('merged', { humanReview: null, status: 'in_review' }));
    expect(screen.getByText('In review')).toBeInTheDocument();
  });

  it('shows no status or actions for a Done ticket without a review, but keeps the branch', () => {
    gitInfoState.data = { ...gitInfo, gitState: 'in_review' };
    renderRow(makeTicket('in_review', { humanReview: null }));
    expect(screen.queryByText('In review')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Rebase…' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'More git actions' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: gitInfo.ticketBranch })).toBeInTheDocument();
  });

  it('lets an admin push main', () => {
    renderRow(makeTicket('not_pushed'));
    const button = screen.getByRole('button', { name: 'Push main' });
    expect(button).toBeEnabled();
    expect(button).not.toHaveAttribute('title');
  });

  it('disables Push main for a member and says who can push', () => {
    sessionState.role = 'member';
    renderRow(makeTicket('not_pushed'));
    const button = screen.getByRole('button', { name: 'Push main' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Ask an admin to push main');
    fireEvent.click(button);
    expect(pushMainMutateAsync).not.toHaveBeenCalled();
  });

  it('loads git info on a card only when it needs action', () => {
    const view = renderRow(makeTicket('merged'), 'card');
    expect(gitInfoEnabled.every((enabled) => !enabled)).toBe(true);
    view.unmount();
    gitInfoEnabled.length = 0;
    renderRow(makeTicket('not_merged'), 'card');
    expect(gitInfoEnabled.at(-1)).toBe(true);
  });

  it('always loads git info in the panel', () => {
    renderRow(makeTicket('merged'));
    expect(gitInfoEnabled.at(-1)).toBe(true);
  });

  it('describes a merge: base branch, pushed and worktree removed', () => {
    gitInfoState.data = { ...gitInfo, gitState: 'merged', worktreeExists: false };
    reposState.data = [{ id: makeTicket('merged').repoId, defaultBranch: 'main', pushAfterMerge: true }];
    renderRow(
      makeTicket('merged', {
        humanReview: { ...makeTicket('merged').humanReview!, mergedBaseBranch: 'release' },
      }),
    );
    expect(screen.getByTestId('git-merged-detail')).toHaveTextContent(
      'release · pushed · worktree removed',
    );
  });

  it('describes a merge with only the parts that apply', () => {
    gitInfoState.data = { ...gitInfo, gitState: 'merged', worktreeExists: true };
    reposState.data = [{ id: makeTicket('merged').repoId, defaultBranch: 'main', pushAfterMerge: false }];
    renderRow(makeTicket('merged'));
    expect(screen.getByTestId('git-merged-detail')).toHaveTextContent(/^main$/);
  });
});
