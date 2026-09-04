import '@testing-library/jest-dom/vitest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ToastProvider } from '../../components/ToastProvider';
import { ApiError } from '../../lib/api';
import type { Ticket } from '../board/useTickets';
import { TicketGitActions } from './TicketGitActions';
import type { TicketGitInfo } from './useTicket';

const rebaseMutateAsync = vi.fn();
const gitInfoState: { data: TicketGitInfo | undefined; isLoading: boolean } = {
  data: undefined,
  isLoading: false,
};

vi.mock('./useTicket', () => ({
  useTicketGitInfo: () => ({
    data: gitInfoState.data,
    isLoading: gitInfoState.isLoading,
  }),
  useMergeTicketBranch: () => ({ mutateAsync: vi.fn(), isPending: false }),
  useRebaseTicketBranch: () => ({
    mutateAsync: rebaseMutateAsync,
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
    projectId: '00000000-0000-0000-0000-000000000002',
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
      screen.queryByRole('button', { name: 'Remove worktree' }),
    ).not.toBeInTheDocument();
  });

  it('shows Merge and other final actions only in wait_for_final_review', () => {
    renderActions(makeTicket({ status: 'wait_for_final_review' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Merge…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Push branch' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create PR' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove worktree' })).toBeInTheDocument();
  });

  it('disables Rebase when worktree is missing', () => {
    gitInfoState.data = { ...baseGitInfo, worktreeExists: false };
    renderActions(makeTicket({ status: 'in_progress' }));

    expect(screen.getByRole('button', { name: 'Rebase…' })).toBeDisabled();
  });

  it('renders nothing without a linked repo', () => {
    const { container } = renderActions(
      makeTicket({ repoId: undefined, status: 'in_progress' }),
    );
    expect(container).toBeEmptyDOMElement();
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
});
