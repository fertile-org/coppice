import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ToastProvider } from '../../components/ToastProvider';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { Ticket } from '../board/useTickets';
import { GitStatusRow } from './GitStatusRow';
import { gitStateCopy, type TicketGitState } from './gitStateCopy';
import type { TicketGitInfo } from './useTicket';

const pushMainMutateAsync = vi.fn();
const gitInfoState: { data: TicketGitInfo | undefined } = { data: undefined };

vi.mock('./useTicket', () => ({
  useTicketGitInfo: () => ({ data: gitInfoState.data, isLoading: false }),
  useMergeTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRebaseTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useResolveConflict: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRemoveWorktree: () => ({ mutateAsync: vi.fn(), isPending: false }),
  usePushTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useCreateTicketPr: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('../repos/useRepos', () => ({
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
    gitInfoState.data = { ...gitInfo };
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
    expect(screen.getByRole('menuitem', { name: 'Push branch' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /^Merge/ })).not.toBeInTheDocument();
  });

  it('offers Merge into… in the dropdown once accepted', () => {
    renderRow(makeTicket('not_merged'));
    fireEvent.click(screen.getByRole('button', { name: 'More git actions' }));
    expect(screen.getByRole('menuitem', { name: 'Merge into…' })).toBeInTheDocument();
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

  it('treats a missing human review as in review', () => {
    gitInfoState.data = { ...gitInfo, gitState: undefined };
    renderRow(makeTicket('merged', { humanReview: null }));
    expect(screen.getByText('In review')).toBeInTheDocument();
  });
});
