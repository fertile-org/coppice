import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ToastProvider } from '../../components/ToastProvider';
import { ApiError } from '../../lib/api';
import type { Ticket } from '../board/useTickets';
import { TicketGitActions } from './TicketGitActions';
import type { TicketGitInfo } from './useTicket';

const rebaseMutateAsync = vi.fn();
const mergeMutateAsync = vi.fn();
const resolveMutateAsync = vi.fn();
const gitInfoState: { data: TicketGitInfo | undefined; isLoading: boolean } = {
  data: undefined,
  isLoading: false,
};

vi.mock('./useTicket', () => ({
  useTicketGitInfo: () => ({
    data: gitInfoState.data,
    isLoading: gitInfoState.isLoading,
  }),
  useMergeTicketBranch: () => ({
    mutateAsync: mergeMutateAsync,
    isPending: false,
  }),
  useRebaseTicketBranch: () => ({
    mutateAsync: rebaseMutateAsync,
    isPending: false,
  }),
  useResolveConflict: () => ({
    mutateAsync: resolveMutateAsync,
    isPending: false,
  }),
  useRemoveWorktree: () => ({ mutateAsync: vi.fn(), isPending: false }),
  usePushTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useCreateTicketPr: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

const baseGitInfo: TicketGitInfo = {
  ticketBranch: 'agent/TICKET-abc',
  worktreePath: '/tmp/worktrees/TICKET-abc-test-repo',
  worktreeExists: true,
  defaultBranch: 'main',
  branches: ['main', 'agent/TICKET-abc'],
  canPush: true,
  canCreatePr: true,
};

function makeTicket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: '00000000-0000-0000-0000-000000000001',
    boardId: '00000000-0000-0000-0000-000000000002',
    repoId: '00000000-0000-0000-0000-000000000003',
    title: 'Test ticket',
    description: 'Ticket description',
    status: 'in_progress',
    createdBy: 'user',
    createdAt: '2026-06-08T00:00:00.000Z',
    updatedAt: '2026-06-08T00:00:00.000Z',
    lastActivityAt: '2026-06-08T00:00:00.000Z',
    ...overrides,
  };
}

function renderActions(ticket: Ticket) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <TicketGitActions ticket={ticket} />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe('TicketGitActions', () => {
  beforeEach(() => {
    rebaseMutateAsync.mockReset();
    mergeMutateAsync.mockReset();
    resolveMutateAsync.mockReset();
    resolveMutateAsync.mockResolvedValue({ run: { id: 'run-1' } });
    gitInfoState.data = { ...baseGitInfo };
    gitInfoState.isLoading = false;
  });

  it('shows Rebase for in_progress when worktree exists, hides Merge', () => {
    renderActions(makeTicket({ status: 'in_progress' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: 'Merge…' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Push branch' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Create PR' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Create PR via API' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Remove worktree' }),
    ).not.toBeInTheDocument();
  });

  it('shows Merge and other final actions only in wait_for_final_review', () => {
    gitInfoState.data = {
      ...baseGitInfo,
      canCreatePr: false,
      prCreateUrl: 'https://github.com/org/repo/compare/main...agent/TICKET-abc?expand=1',
    };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Merge…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Push branch' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open compare URL' })).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Create PR via API' }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove worktree' })).toBeInTheDocument();
  });

  it('shows API Create PR only when canCreatePr is true', () => {
    gitInfoState.data = {
      ...baseGitInfo,
      canCreatePr: true,
      prCreateUrl: 'https://github.com/org/repo/compare/main...agent/TICKET-abc?expand=1',
    };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    expect(screen.getByRole('button', { name: 'Open compare URL' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create PR via API' })).toBeInTheDocument();
  });

  it('disables Rebase when worktree is missing', () => {
    gitInfoState.data = { ...baseGitInfo, worktreeExists: false };
    renderActions(makeTicket({ status: 'in_progress' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeDisabled();
  });

  it('renders nothing without a linked repo', () => {
    renderActions(makeTicket({ repoId: undefined, status: 'in_progress' }));
    expect(screen.queryByText('Git actions')).not.toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows the accept-again message when merge is refused', async () => {
    const message =
      "Not merged. Accept again so Coppice knows which commit you reviewed.";
    mergeMutateAsync.mockRejectedValue(
      new ApiError(409, JSON.stringify({ message })),
    );

    renderActions(makeTicket({ status: 'done' }));
    fireEvent.click(screen.getByRole('button', { name: 'Merge…' }));
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(screen.getByRole('button', { name: /^Merge$/ }));

    await waitFor(() => {
      expect(dialog).toHaveTextContent(message);
    });
  });

  it('shows toast and inline error when rebase fails', async () => {
    const conflictMessage =
      'Rebase conflict — aborted. Conflicting paths: README.md.';
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(400, JSON.stringify({ message: conflictMessage })),
    );

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));

    expect(
      await screen.findByRole('dialog', { name: /rebase ticket branch/i }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Rebase' }));

    await waitFor(() => {
      expect(screen.getByTestId('rebase-inline-error')).toHaveTextContent(
        conflictMessage,
      );
    });
    const toasts = screen.getAllByRole('status');
    expect(toasts.some((el) => el.textContent?.includes(conflictMessage))).toBe(
      true,
    );
  });

  const conflictMessage =
    "Couldn't rebase onto main because these files conflict: README.md. Nothing was changed.";
  const rereviewNote =
    "When the conflicts are resolved, the ticket goes back to In Review. You'll need to accept it again before it merges.";

  function conflictBody(
    overrides: Record<string, unknown> = {},
    message = conflictMessage,
  ) {
    return JSON.stringify({
      message,
      conflict: {
        operation: 'rebase',
        baseBranch: 'main',
        files: ['README.md'],
        canAskAssignee: true,
        askLabel: 'Ask Ada to resolve',
        rereviewNote,
        ...overrides,
      },
    });
  }

  it('offers to ask the assignee from the rebase dialog and the drawer', async () => {
    rebaseMutateAsync.mockRejectedValue(new ApiError(400, conflictBody()));

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const dialog = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rebase' }));

    await waitFor(() => {
      expect(dialog).toHaveTextContent(conflictMessage);
    });
    expect(dialog).toHaveTextContent('README.md');
    expect(dialog).toHaveTextContent(rereviewNote);
    expect(resolveMutateAsync).not.toHaveBeenCalled();

    const ask = within(dialog).getByRole('button', { name: 'Ask Ada to resolve' });
    const cancel = within(dialog).getByRole('button', { name: 'Cancel' });
    expect(ask.parentElement).toBe(cancel.parentElement);
    // The dialog hides the drawer from assistive tech, so include that button.
    expect(screen.getAllByTestId('git-conflict')).toHaveLength(2);
    expect(
      screen.getAllByRole('button', { name: 'Ask Ada to resolve', hidden: true }),
    ).toHaveLength(2);

    fireEvent.click(ask);
    fireEvent.click(ask);
    await waitFor(() => {
      expect(resolveMutateAsync).toHaveBeenCalledTimes(1);
    });
    expect(resolveMutateAsync).toHaveBeenCalledWith({
      baseBranch: 'main',
      files: ['README.md'],
    });
  });

  it('offers to ask the assignee from the merge dialog', async () => {
    const mergeMessage =
      "Couldn't merge into main because these files conflict: README.md. Nothing was changed.";
    mergeMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody(
          {
            operation: 'merge',
          },
          mergeMessage,
        ),
      ),
    );

    renderActions(makeTicket({ status: 'done' }));
    fireEvent.click(screen.getByRole('button', { name: 'Merge…' }));
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(within(dialog).getByRole('button', { name: /^Merge$/ }));

    const ask = await within(dialog).findByRole('button', {
      name: 'Ask Ada to resolve',
    });
    expect(dialog).toHaveTextContent(mergeMessage);
    expect(ask.parentElement).toBe(
      within(dialog).getByRole('button', { name: 'Cancel' }).parentElement,
    );
    expect(resolveMutateAsync).not.toHaveBeenCalled();
  });

  it('hides the ask button when there is no assignee', async () => {
    const reason = 'Assign an agent to this ticket to resolve the conflicts.';
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody({
          canAskAssignee: false,
          askLabel: undefined,
          unavailableReason: reason,
        }),
      ),
    );

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const dialog = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rebase' }));

    await waitFor(() => {
      expect(dialog).toHaveTextContent(reason);
    });
    expect(screen.queryByRole('button', { name: /Ask .+ to resolve/ })).not.toBeInTheDocument();
    expect(resolveMutateAsync).not.toHaveBeenCalled();
  });

  it("hides the ask button when the assignee's connector is not ready", async () => {
    const reason = "Ada's connector isn't ready. Check it in Tools → Connectors.";
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody({
          canAskAssignee: false,
          askLabel: undefined,
          unavailableReason: reason,
        }),
      ),
    );

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Rebase' }));

    expect(await screen.findAllByText(reason)).not.toHaveLength(0);
    expect(screen.queryByRole('button', { name: /Ask .+ to resolve/ })).not.toBeInTheDocument();
  });
});
