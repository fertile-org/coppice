import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmProvider } from '../../components/ui/confirm-dialog';
import type { SplitProgress } from '../board/useTickets';
import { PLAN_COPY } from './copy';
import { SplitParentTracker } from './SplitParentTracker';

const mocks = vi.hoisted(() => ({ apiFetch: vi.fn() }));

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch: mocks.apiFetch,
}));

vi.mock('../tickets/useTicket', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../tickets/useTicket')>();
  return {
    ...actual,
    useTicketGitInfo: () => ({
      data: {
        ticketBranch: 'agent/TICKET-abc',
        prUrl: 'https://github.com/acme/coppice/pull/9',
      },
      isLoading: false,
    }),
  };
});

const reviewId = '00000000-0000-0000-0000-000000000022';
const queuedId = '00000000-0000-0000-0000-000000000023';

function progress(autoStart = true): SplitProgress {
  return {
    parentTicketId: '00000000-0000-0000-0000-000000000001',
    parentTitle: 'Re-design the subticket UIUX',
    count: 2,
    mergedCount: 0,
    autoStart,
    activeChildId: reviewId,
    nextActor: 'you',
    children: [
      {
        id: reviewId,
        title: 'Plan Review shows proposed sub-tickets',
        status: 'wait_for_final_review',
        position: 2,
        archived: false,
        merged: false,
      },
      {
        id: queuedId,
        title: 'Parent tracker panel',
        status: 'backlog',
        position: 3,
        archived: false,
        merged: false,
      },
    ],
  };
}

function renderTracker(autoStart = true) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function wrap(node: ReactNode) {
    return (
      <MemoryRouter>
        <QueryClientProvider client={client}>
          <ConfirmProvider>{node}</ConfirmProvider>
        </QueryClientProvider>
      </MemoryRouter>
    );
  }
  return render(
    wrap(
      <SplitParentTracker
        ticketId="00000000-0000-0000-0000-000000000001"
        boardId="board-1"
        repoId="repo-1"
        progress={progress(autoStart)}
      />,
    ),
  );
}

describe('SplitParentTracker', () => {
  beforeEach(() => {
    mocks.apiFetch.mockReset();
    mocks.apiFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: '00000000-0000-0000-0000-000000000001', boardId: 'board-1' }),
    });
  });

  it('points Your turn at the sub-ticket that is waiting for review', () => {
    renderTracker();
    expect(screen.getByText(PLAN_COPY.yourTurnReview(2))).toBeVisible();
    expect(screen.getByText('Plan Review shows proposed sub-tickets')).toBeVisible();
    expect(screen.getByText('Parent tracker panel')).toBeVisible();
    expect(screen.getByText('Wait for Human Review')).toBeVisible();
    const review = screen.getByRole('link', { name: PLAN_COPY.reviewCode });
    expect(review).toHaveAttribute('href', expect.stringContaining(`ticketId=${reviewId}`));
    expect(review.getAttribute('href')).not.toContain(queuedId);
  });

  it('pauses, resumes, and starts the next sub-ticket', async () => {
    const { unmount } = renderTracker(true);
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.splitPause }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/00000000-0000-0000-0000-000000000001/split/pause',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    unmount();

    mocks.apiFetch.mockClear();
    renderTracker(false);
    expect(screen.getByRole('button', { name: PLAN_COPY.splitResume })).toBeVisible();
    expect(screen.queryByRole('button', { name: PLAN_COPY.splitPause })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.splitStartNext }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/00000000-0000-0000-0000-000000000001/split/start-next',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });

  it('names the branch and pull request before recording a merge elsewhere', async () => {
    renderTracker();
    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.splitMergedElsewhere }));
    expect(
      await screen.findByText(PLAN_COPY.splitMergedElsewhereBody(
        'agent/TICKET-abc',
        'https://github.com/acme/coppice/pull/9',
      )),
    ).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mocks.apiFetch).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: PLAN_COPY.splitMergedElsewhere }));
    fireEvent.click(await screen.findByRole('button', { name: PLAN_COPY.splitRecordMerge }));
    await waitFor(() =>
      expect(mocks.apiFetch).toHaveBeenCalledWith(
        '/api/tickets/00000000-0000-0000-0000-000000000001/split/merged-elsewhere',
        expect.objectContaining({ method: 'POST' }),
      ),
    );
  });
});
