import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { ApiError } from '../../lib/api';
import { TicketCard } from './TicketCard';
import type { Ticket } from './useTickets';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

const ticketId = '00000000-0000-0000-0000-000000000010';

const acceptedReview = {
  headSha: 'abc1234deadbeef',
  shortSha: 'abc1234',
  stale: false,
  merged: false,
  commentIds: [],
  runIds: [],
};

function makeTicket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: ticketId,
    boardId: '00000000-0000-0000-0000-000000000011',
    repoId: '00000000-0000-0000-0000-000000000012',
    title: 'Ship the accepted branch',
    description: '',
    status: 'done',
    createdBy: 'user',
    createdAt: '2026-10-14T00:00:00.000Z',
    updatedAt: '2026-10-14T00:00:00.000Z',
    lastActivityAt: '2026-10-14T00:00:00.000Z',
    humanReview: acceptedReview,
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function renderCard(ticket: Ticket, onOpen = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <DndContext>
          <TicketCard ticket={ticket} onOpen={onOpen} />
        </DndContext>
      </ToastProvider>
    </QueryClientProvider>,
  );
  return onOpen;
}

describe('Done card accepted branch', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockImplementation(async (path: string) => {
      if (String(path).endsWith('/git-info')) {
        return jsonResponse({
          ticketBranch: 'agent/accepted',
          worktreePath: '/tmp/wt',
          worktreeExists: true,
          defaultBranch: 'main',
          branches: ['main', 'agent/accepted'],
          canCreatePr: true,
        });
      }
      return jsonResponse({});
    });
  });

  it('shows the accepted-not-merged label and Merge button on a Done card', async () => {
    renderCard(makeTicket());

    expect(
      await screen.findByText('[placeholder] Accepted, not merged'),
    ).toBeVisible();
    expect(screen.getByRole('button', { name: 'Merge…' })).toBeVisible();
    expect(
      await screen.findByRole('button', { name: 'Create PR via API' }),
    ).toBeVisible();
    expect(
      screen.queryByRole('button', { name: '[placeholder] Merge' }),
    ).toBeNull();
  });

  it('hides the label when the accepted branch is already merged', async () => {
    renderCard(
      makeTicket({
        humanReview: { ...acceptedReview, merged: true },
      }),
    );

    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
    expect(screen.queryByRole('button', { name: 'Merge…' })).toBeNull();
    expect(
      await screen.findByRole('button', { name: 'Create PR via API' }),
    ).toBeVisible();
  });

  it('hides the label when the Done card was not accepted', () => {
    renderCard(makeTicket({ humanReview: null }));

    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
  });

  it('hides the label outside Done, when acceptance is stale, when archived, and without a repository', () => {
    const { unmount } = render(
      <DndContext>
        <TicketCard
          ticket={makeTicket({ status: 'wait_for_final_review' })}
          onOpen={vi.fn()}
        />
      </DndContext>,
    );
    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
    unmount();

    renderCard(
      makeTicket({
        humanReview: { ...acceptedReview, stale: true },
      }),
    );
    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
  });

  it('hides the label for an archived Done card and a Done card with no repository', () => {
    const { unmount } = render(
      <DndContext>
        <TicketCard
          ticket={makeTicket({ archivedAt: '2026-10-14T00:00:00.000Z' })}
          onOpen={vi.fn()}
        />
      </DndContext>,
    );
    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
    unmount();

    renderCard(makeTicket({ repoId: undefined }));
    expect(
      screen.queryByText('[placeholder] Accepted, not merged'),
    ).toBeNull();
  });

  it('merges into the default branch and clears the label after success', async () => {
    let releaseMerge: (response: Response) => void = () => {};
    const mergeStarted = new Promise<void>((resolveStarted) => {
      apiFetch.mockImplementation(async (path: string, init?: RequestInit) => {
        if (String(path).endsWith('/git-info')) {
          return jsonResponse({
            ticketBranch: 'agent/accepted',
            worktreePath: '/tmp/wt',
            worktreeExists: true,
            defaultBranch: 'main',
            branches: ['main'],
            canCreatePr: true,
          });
        }
        if (
          String(path) === `/api/tickets/${ticketId}/merge-branch` &&
          init?.method === 'POST'
        ) {
          resolveStarted();
          return new Promise<Response>((resolve) => {
            releaseMerge = resolve;
          });
        }
        return jsonResponse({});
      });
    });

    const onOpen = renderCard(makeTicket());
    const mergeButton = await screen.findByRole('button', { name: 'Merge…' });
    await waitFor(() => expect(mergeButton).toBeEnabled());

    fireEvent.click(mergeButton);
    expect(onOpen).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(within(dialog).getByRole('button', { name: /^Merge$/ }));

    await mergeStarted;
    await waitFor(() => {
      expect(within(dialog).getByRole('button', { name: 'Merging…' })).toBeDisabled();
    });
    expect(apiFetch).toHaveBeenCalledWith(
      `/api/tickets/${ticketId}/merge-branch`,
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ baseBranch: 'main' }),
      }),
    );

    releaseMerge(
      jsonResponse({
        merge: {
          baseBranch: 'main',
          ticketBranch: 'agent/accepted',
          headSha: 'abc1234deadbeef',
          message: 'Merged `agent/accepted` into `main`',
        },
      }),
    );

    await waitFor(() => {
      expect(
        screen.queryByText('[placeholder] Accepted, not merged'),
      ).toBeNull();
    });
    expect(await screen.findByText('Merged `agent/accepted` into `main`')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Create PR via API' })).toBeVisible();
    expect(
      apiFetch.mock.calls.some((call) => String(call[0]).endsWith('/status')),
    ).toBe(false);
  });

  it('shows conflicting files and a short error and leaves the card in place', async () => {
    apiFetch.mockImplementation(async (path: string, init?: RequestInit) => {
      if (String(path).endsWith('/git-info')) {
        return jsonResponse({
          ticketBranch: 'agent/accepted',
          worktreePath: '/tmp/wt',
          worktreeExists: true,
          defaultBranch: 'main',
          branches: ['main'],
        });
      }
      if (
        String(path) === `/api/tickets/${ticketId}/merge-branch` &&
        init?.method === 'POST'
      ) {
        throw new ApiError(
          400,
          JSON.stringify({
            message:
              "Couldn't merge into main because these files conflict: src/app.ts, README.md. Nothing was changed.",
            conflict: {
              operation: 'merge',
              baseBranch: 'main',
              files: ['README.md', 'src/app.ts'],
              canAskAssignee: false,
              rereviewNote:
                "When the conflicts are resolved, the ticket goes back to In Review. You'll need to accept it again before it merges.",
            },
          }),
        );
      }
      return jsonResponse({});
    });

    renderCard(makeTicket());
    const mergeButton = await screen.findByRole('button', { name: 'Merge…' });
    await waitFor(() => expect(mergeButton).toBeEnabled());
    fireEvent.click(mergeButton);
    const dialog = await screen.findByRole('dialog', { name: 'Merge ticket branch' });
    fireEvent.click(within(dialog).getByRole('button', { name: /^Merge$/ }));

    expect(
      await within(dialog).findByText(/README\.md/),
    ).toBeVisible();
    expect(within(dialog).getByText(/src\/app\.ts/)).toBeVisible();
    expect(
      screen.getByText('[placeholder] Accepted, not merged'),
    ).toBeVisible();
    expect(screen.getByText('Ship the accepted branch')).toBeVisible();
    expect(
      apiFetch.mock.calls.some((call) => String(call[0]).endsWith('/status')),
    ).toBe(false);
  });

  it('creates a pull request from a Done card with the existing button', async () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null);
    apiFetch.mockImplementation(async (path: string, init?: RequestInit) => {
      if (String(path).endsWith('/git-info')) {
        return jsonResponse({
          ticketBranch: 'agent/accepted',
          worktreePath: '/tmp/wt',
          worktreeExists: true,
          defaultBranch: 'main',
          branches: ['main'],
          canCreatePr: true,
        });
      }
      if (
        String(path) === `/api/tickets/${ticketId}/create-pr` &&
        init?.method === 'POST'
      ) {
        return jsonResponse({
          pullRequest: { prUrl: 'https://github.com/org/repo/pull/7', number: 7, title: 'Ship' },
        });
      }
      return jsonResponse({});
    });

    renderCard(makeTicket({ humanReview: { ...acceptedReview, merged: true } }));
    const createPr = await screen.findByRole('button', { name: 'Create PR via API' });
    fireEvent.click(createPr);

    await waitFor(() => {
      expect(apiFetch).toHaveBeenCalledWith(
        `/api/tickets/${ticketId}/create-pr`,
        expect.objectContaining({ method: 'POST' }),
      );
    });
    expect(await screen.findByText('PR #7 created')).toBeVisible();
    open.mockRestore();
  });
});
