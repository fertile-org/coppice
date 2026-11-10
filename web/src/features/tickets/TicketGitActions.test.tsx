import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ToastProvider } from '../../components/ToastProvider';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import { ApiError } from '../../lib/api';
import type { Ticket } from '../board/useTickets';
import { TicketGitActions } from './TicketGitActions';
import type { TicketGitInfo } from './useTicket';

const rebaseMutateAsync = vi.fn();
const mergeMutateAsync = vi.fn();
const resolveMutateAsync = vi.fn();
const removeWorktreeMutateAsync = vi.fn();
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
  useRemoveWorktree: () => ({
    mutateAsync: removeWorktreeMutateAsync,
    isPending: false,
  }),
  usePushTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useCreateTicketPr: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

vi.mock('../repos/useRepos', () => ({
  usePushDefaultBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));

const baseGitInfo: TicketGitInfo = {
  ticketBranch: 'agent/TICKET-abc',
  worktreePath: '/tmp/worktrees/TICKET-abc-test-repo',
  worktreeExists: true,
  defaultBranch: 'main',
  branches: ['main', 'agent/TICKET-abc'],
  canPush: true,
  canCreatePr: true,
  gitState: 'in_review',
};

function openMenu() {
  fireEvent.click(screen.getByRole('button', { name: 'More git actions' }));
}

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
  function Tree({ ticket }: { ticket: Ticket }) {
    return (
      <QueryClientProvider client={client}>
        <ToastProvider>
          <ConfirmProvider>
            <TicketGitActions ticket={ticket} />
          </ConfirmProvider>
        </ToastProvider>
      </QueryClientProvider>
    );
  }
  const view = render(<Tree ticket={ticket} />);
  return {
    ...view,
    rerenderTicket: (next: Ticket) => view.rerender(<Tree ticket={next} />),
  };
}

async function expectResolveToast(agentName: string) {
  await waitFor(() => {
    expect(
      screen
        .getAllByRole('status')
        .some((el) => el.textContent?.includes(`${agentName} is resolving the conflict`)),
    ).toBe(true);
  });
  expect(
    screen.queryByRole('heading', { name: `${agentName} is resolving the conflict` }),
  ).not.toBeInTheDocument();
}

describe('TicketGitActions', () => {
  beforeEach(() => {
    rebaseMutateAsync.mockReset();
    mergeMutateAsync.mockReset();
    resolveMutateAsync.mockReset();
    removeWorktreeMutateAsync.mockReset();
    removeWorktreeMutateAsync.mockResolvedValue(undefined);
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

  it('lists the final actions in the menu and hides Merge until accepted', () => {
    gitInfoState.data = {
      ...baseGitInfo,
      canCreatePr: false,
      prCreateUrl: 'https://github.com/org/repo/compare/main...agent/TICKET-abc?expand=1',
    };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeInTheDocument();
    openMenu();
    expect(screen.getByRole('menuitem', { name: 'Push branch' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Open compare URL' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Remove worktree' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /^Merge/ })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('menuitem', { name: 'Create PR via API' }),
    ).not.toBeInTheDocument();
  });

  it('keeps Remove worktree off while an agent run is active', () => {
    renderActions(makeTicket({ status: 'wait_for_final_review', hasActiveRun: true }));
    openMenu();

    expect(screen.getByRole('menuitem', { name: 'Remove worktree' })).toBeDisabled();
  });

  it('shows Merge and Create PR for a Done ticket that is not merged', () => {
    gitInfoState.data = { ...baseGitInfo, canCreatePr: true, gitState: 'not_merged' };
    renderActions(makeTicket({ status: 'done' }));

    expect(screen.getByRole('button', { name: 'Merge' })).toBeInTheDocument();
    openMenu();
    expect(screen.getByRole('menuitem', { name: 'Create PR via API' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Rebase…' })).toBeInTheDocument();
  });

  it('shows API Create PR only when canCreatePr is true', () => {
    gitInfoState.data = {
      ...baseGitInfo,
      canCreatePr: true,
      prCreateUrl: 'https://github.com/org/repo/compare/main...agent/TICKET-abc?expand=1',
    };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));
    openMenu();

    expect(screen.getByRole('menuitem', { name: 'Open compare URL' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Create PR via API' })).toBeInTheDocument();
  });

  it('disables Rebase when worktree is missing', () => {
    gitInfoState.data = { ...baseGitInfo, worktreeExists: false };
    renderActions(makeTicket({ status: 'in_progress' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeDisabled();
  });

  it('renders nothing without a linked repo', () => {
    renderActions(makeTicket({ repoId: undefined, status: 'in_progress' }));
    expect(screen.queryByText('Git')).not.toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows the accept-again message when merge is refused', async () => {
    const message =
      "Not merged. Accept again so Coppice knows which commit you reviewed.";
    mergeMutateAsync.mockRejectedValue(
      new ApiError(409, JSON.stringify({ message })),
    );

    gitInfoState.data = { ...baseGitInfo, gitState: 'not_merged' };
    renderActions(makeTicket({ status: 'done' }));
    fireEvent.click(screen.getByRole('button', { name: 'Merge' }));
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(within(dialog).getByRole('button', { name: /^Merge$/ }));

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
  const adaId = '00000000-0000-0000-0000-0000000000aa';
  const beaId = '00000000-0000-0000-0000-0000000000bb';
  const adaAgent = {
    id: adaId,
    name: 'Ada',
    askLabel: 'Ask Ada to resolve',
  };
  const beaAgent = {
    id: beaId,
    name: 'Bea',
    askLabel: 'Ask Bea to resolve',
  };

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
        defaultAgentId: adaId,
        agents: [adaAgent],
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
    expect(within(dialog).getByRole('button', { name: 'Rebase again' })).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Who should resolve it?')).toHaveValue(adaId);
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
      operation: 'rebase',
      agentId: adaId,
    });
    await expectResolveToast('Ada');
    expect(screen.queryByRole('dialog', { name: /rebase ticket branch/i })).not.toBeInTheDocument();
  });

  it('sends the selected agent and updates the ask label', async () => {
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody({
          agents: [adaAgent, beaAgent],
        }),
      ),
    );

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const dialog = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rebase' }));

    const picker = await within(dialog).findByLabelText('Who should resolve it?');
    fireEvent.change(picker, { target: { value: beaId } });
    const ask = within(dialog).getByRole('button', { name: 'Ask Bea to resolve' });
    fireEvent.click(ask);
    await waitFor(() => {
      expect(resolveMutateAsync).toHaveBeenCalledWith({
        baseBranch: 'main',
        files: ['README.md'],
        operation: 'rebase',
        agentId: beaId,
      });
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

    gitInfoState.data = { ...baseGitInfo, gitState: 'not_merged' };
    const view = renderActions(makeTicket({ status: 'done' }));
    fireEvent.click(screen.getByRole('button', { name: 'Merge' }));
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(within(dialog).getByRole('button', { name: /^Merge$/ }));

    const ask = await within(dialog).findByRole('button', {
      name: 'Ask Ada to resolve',
    });
    expect(dialog).toHaveTextContent(mergeMessage);
    expect(ask.parentElement).toBe(
      within(dialog).getByRole('button', { name: 'Cancel' }).parentElement,
    );
    expect(within(dialog).getByRole('button', { name: 'Merge again' })).toBeInTheDocument();
    fireEvent.click(ask);
    await waitFor(() => {
      expect(resolveMutateAsync).toHaveBeenCalledWith({
        baseBranch: 'main',
        files: ['README.md'],
        operation: 'merge',
        agentId: adaId,
      });
    });
    await expectResolveToast('Ada');
    expect(screen.queryByRole('dialog', { name: 'Merge ticket branch' })).not.toBeInTheDocument();

    gitInfoState.data = { ...baseGitInfo, gitState: 'in_review' };
    view.rerenderTicket(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const rebase = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    expect(rebase).not.toHaveTextContent('Ada is resolving the conflict');
    expect(within(rebase).getByRole('button', { name: 'Rebase' })).toBeInTheDocument();
  });

  it('toasts after ask from the drawer and keeps Rebase on the form', async () => {
    rebaseMutateAsync.mockRejectedValue(new ApiError(400, conflictBody()));

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const dialog = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rebase' }));

    await waitFor(() => {
      expect(dialog).toHaveTextContent(conflictMessage);
    });
    const drawerAsk = screen
      .getAllByRole('button', { name: 'Ask Ada to resolve', hidden: true })
      .find((button) => !dialog.contains(button));
    expect(drawerAsk).toBeTruthy();
    fireEvent.click(drawerAsk!);

    await waitFor(() => {
      expect(resolveMutateAsync).toHaveBeenCalledTimes(1);
    });
    await expectResolveToast('Ada');
    expect(screen.queryByRole('dialog', { name: /rebase ticket branch/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const rebase = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    expect(rebase).not.toHaveTextContent('Ada is resolving the conflict');
    expect(within(rebase).getByRole('button', { name: 'Rebase' })).toBeInTheDocument();
  });

  it('hides the ask button when no agent can resolve', async () => {
    const reason = 'No agent on this board can resolve the conflicts.';
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody({
          canAskAssignee: false,
          askLabel: undefined,
          defaultAgentId: undefined,
          agents: [],
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
    expect(screen.queryByLabelText('Who should resolve it?')).not.toBeInTheDocument();
    expect(resolveMutateAsync).not.toHaveBeenCalled();
  });

  it("disables the ask button when the selected agent's connector is not ready", async () => {
    const reason = "Ada's connector isn't ready. Check it in System → Connectors.";
    rebaseMutateAsync.mockRejectedValue(
      new ApiError(
        400,
        conflictBody({
          canAskAssignee: true,
          agents: [
            { ...adaAgent, unavailableReason: reason },
            beaAgent,
          ],
          unavailableReason: reason,
        }),
      ),
    );

    renderActions(makeTicket({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('button', { name: 'Rebase…' }));
    const dialog = await screen.findByRole('dialog', { name: /rebase ticket branch/i });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rebase' }));

    expect(await screen.findAllByText(reason)).not.toHaveLength(0);
    expect(within(dialog).getByRole('button', { name: 'Ask Ada to resolve' })).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText('Who should resolve it?'), {
      target: { value: beaId },
    });
    expect(within(dialog).getByRole('button', { name: 'Ask Bea to resolve' })).toBeEnabled();
  });

  it('cancels remove worktree without calling the API', async () => {
    gitInfoState.data = { ...baseGitInfo, gitState: 'worktree_kept' };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    fireEvent.click(screen.getByRole('button', { name: 'Remove worktree' }));
    const dialog = await screen.findByRole('dialog', {
      name: /Remove worktree\?/,
    });
    expect(dialog).toHaveTextContent(baseGitInfo.worktreePath);
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));

    expect(removeWorktreeMutateAsync).not.toHaveBeenCalled();
  });

  it('removes the worktree after confirm', async () => {
    gitInfoState.data = { ...baseGitInfo, gitState: 'worktree_kept' };
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    fireEvent.click(screen.getByRole('button', { name: 'Remove worktree' }));
    const dialog = await screen.findByRole('dialog', {
      name: /Remove worktree\?/,
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Remove worktree' }));

    await waitFor(() => expect(removeWorktreeMutateAsync).toHaveBeenCalled());
  });
});
