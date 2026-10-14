import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RestartChoice } from './RestartChoice';

const ticketId = '00000000-0000-0000-0000-000000000001';
const boardId = '00000000-0000-0000-0000-000000000002';

function renderChoice() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <RestartChoice ticketId={ticketId} boardId={boardId} />
    </QueryClientProvider>,
  );
}

describe('RestartChoice', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('shows resume and discard, and confirms before discarding', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith('/restart') && (!init || init.method === undefined)) {
        return new Response(
          JSON.stringify({
            pending: true,
            resumeLabel: 'Keep the changes',
            otherLabel: 'Start over',
            confirm: "Delete the changes to 4 files and start over? You can't undo this.",
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ pending: false }), { status: 200 });
    });
    vi.stubGlobal('fetch', fetchMock);

    renderChoice();
    expect(await screen.findByRole('button', { name: 'Keep the changes' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Start over' }));
    const dialog = await screen.findByRole('dialog');
    expect(
      within(dialog).getByText(
        "Delete the changes to 4 files and start over? You can't undo this.",
      ),
    ).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalledWith(
      expect.stringContaining('/restart/discard'),
      expect.anything(),
    );
    fireEvent.click(within(dialog).getByRole('button', { name: 'Start over' }));
    await vi.waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/tickets/${ticketId}/restart/discard`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('resumes and cancels a clean run without a confirm', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith('/restart/resume') || url.endsWith('/restart/cancel')) {
        return new Response(JSON.stringify({ pending: false }), { status: 200 });
      }
      return new Response(
        JSON.stringify({
          pending: true,
          resumeLabel: 'Keep the changes',
          otherLabel: 'Cancel',
          confirm: null,
        }),
        { status: 200 },
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    renderChoice();
    fireEvent.click(await screen.findByRole('button', { name: 'Keep the changes' }));
    await vi.waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/tickets/${ticketId}/restart/resume`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await vi.waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/tickets/${ticketId}/restart/cancel`,
        expect.objectContaining({ method: 'POST' }),
      ),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
