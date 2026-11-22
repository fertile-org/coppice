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

  it('renders the main action for actionable git states', () => {
    for (const state of withButton) {
      const view = renderRow(makeTicket(state));
      expect(
        screen.getByRole('button', { name: gitStateCopy[state].primary! }),
      ).toBeInTheDocument();
      view.unmount();
    }
  });

  it('shows no primary git actions once merged or after a branch change', () => {
    const view = renderRow(makeTicket('changed_after_review'));
    for (const name of ['Rebase…', 'Merge', 'Ask agent', 'Retry merge', 'Push main']) {
      expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    }
    view.unmount();
    renderRow(makeTicket('merged'));
    expect(screen.queryByRole('button', { name: 'More git actions' })).not.toBeInTheDocument();
  });

  it('pushes main and refreshes ticket queries', async () => {
    pushMainMutateAsync.mockResolvedValue({ message: 'Pushed main' });
    const ticket = makeTicket('not_pushed');
    renderRow(ticket);
    fireEvent.click(screen.getByRole('button', { name: 'Push main' }));
    expect(pushMainMutateAsync).toHaveBeenCalledTimes(1);
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

  it('handles tickets without human review', () => {
    gitInfoState.data = { ...gitInfo, gitState: undefined };
    const beforeDone = renderRow(
      makeTicket('merged', { humanReview: null, status: 'in_review' }),
    );
    expect(screen.getByText('No action')).toBeInTheDocument();
    beforeDone.unmount();
    gitInfoState.data = { ...gitInfo, gitState: 'in_review' };
    renderRow(makeTicket('in_review', { humanReview: null }));
    expect(screen.queryByRole('button', { name: 'More git actions' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: gitInfo.ticketBranch })).toBeInTheDocument();
  });

  it('gates Push main on admin role', () => {
    sessionState.role = 'member';
    const member = renderRow(makeTicket('not_pushed'));
    const button = screen.getByRole('button', { name: 'Push main' });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(pushMainMutateAsync).not.toHaveBeenCalled();
    member.unmount();

    sessionState.role = 'admin';
    renderRow(makeTicket('not_pushed'));
    expect(screen.getByRole('button', { name: 'Push main' })).toBeEnabled();
  });

});
