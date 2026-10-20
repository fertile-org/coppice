import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BoardSettingsButton, BoardSettingsDrawer } from './BoardSettingsDrawer';
import { ReviewWaitingChip } from './ReviewWaitingChip';
import { useReviewCapacity } from './useReviewCapacity';

const apiFetch = vi.hoisted(() => vi.fn());

vi.mock('../../lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/api')>()),
  apiFetch,
}));

const invalid =
  'Enter a whole number of 1 or more, or leave it empty for no cap.';

const board = {
  reviewCap: 2,
  countInReview: false,
  waitingOnReview: '2 of 2 waiting for review',
  boardSettingsLabel: 'Board settings',
  reviewCapLabel: 'Review cap',
  reviewCapHint:
    'How many tickets can wait for your review at once. Leave it empty for no cap.',
  countInReviewLabel: 'Also count In Review',
  reviewCapInvalid: invalid,
};

function Harness() {
  const capacity = useReviewCapacity('board-1');
  const [open, setOpen] = useState(false);
  return (
    <>
      <ReviewWaitingChip saved={capacity.saved} />
      <BoardSettingsButton saved={capacity.saved} onClick={() => setOpen(true)} />
      <BoardSettingsDrawer
        open={open}
        boardId="board-1"
        capacity={capacity}
        onClose={() => setOpen(false)}
      />
    </>
  );
}

function renderSettings() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
}

function putCalls() {
  return apiFetch.mock.calls.filter(
    ([url]) => url === '/api/boards/board-1/review-capacity',
  );
}

function expectPut(body: { reviewCap: number | null; countInReview: boolean }) {
  return waitFor(() => {
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/boards/board-1/review-capacity',
      expect.objectContaining({ method: 'PUT', body: JSON.stringify(body) }),
    );
  });
}

async function openSettings() {
  fireEvent.click(await screen.findByRole('button', { name: 'Board settings' }));
  return screen.getByRole('dialog', { name: 'Board settings' });
}

describe('BoardSettingsDrawer', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockImplementation(async (_url: string, init?: RequestInit) => ({
      json: async () =>
        init?.method === 'PUT'
          ? { ...board, ...JSON.parse(String(init.body)), waitingOnReview: null }
          : board,
    }));
  });

  it('shows the waiting chip and keeps the cap field in the closed drawer', async () => {
    renderSettings();
    expect(
      await screen.findByTestId('review-waiting-chip'),
    ).toHaveTextContent('2 of 2 waiting for review');
    expect(screen.queryByText('Review cap')).not.toBeInTheDocument();

    const dialog = await openSettings();
    expect(dialog).toHaveTextContent('Board settings');
    const cap = screen.getByLabelText('Review cap');
    expect(cap).toHaveValue(2);
    expect(cap).toHaveFocus();
    expect(screen.getByLabelText('Also count In Review')).not.toBeChecked();
    expect(
      screen.getByText(
        'How many tickets can wait for your review at once. Leave it empty for no cap.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
  });

  it('saves a cap on blur without inventing its own wording', async () => {
    renderSettings();
    await openSettings();
    const cap = screen.getByLabelText('Review cap');
    fireEvent.change(cap, { target: { value: '4' } });
    fireEvent.blur(cap);
    await expectPut({ reviewCap: 4, countInReview: false });
    await waitFor(() => {
      expect(screen.queryByTestId('review-waiting-chip')).not.toBeInTheDocument();
    });
  });

  it('sends one save for blur followed by Done', async () => {
    renderSettings();
    await openSettings();
    const cap = screen.getByLabelText('Review cap');
    fireEvent.change(cap, { target: { value: '5' } });
    fireEvent.blur(cap);
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await expectPut({ reviewCap: 5, countInReview: false });
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Board settings' }),
      ).not.toBeInTheDocument();
    });
    expect(putCalls()).toHaveLength(1);
  });

  it('saves a typed cap on Done and shows it when reopened', async () => {
    renderSettings();
    await openSettings();
    fireEvent.change(screen.getByLabelText('Review cap'), {
      target: { value: '6' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await expectPut({ reviewCap: 6, countInReview: false });
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Board settings' }),
      ).not.toBeInTheDocument();
    });
    await openSettings();
    expect(screen.getByLabelText('Review cap')).toHaveValue(6);
    expect(putCalls()).toHaveLength(1);
  });

  it('saves no cap on Escape after clearing the field', async () => {
    renderSettings();
    const dialog = await openSettings();
    fireEvent.change(screen.getByLabelText('Review cap'), {
      target: { value: '' },
    });
    fireEvent.keyDown(dialog, { key: 'Escape' });
    await expectPut({ reviewCap: null, countInReview: false });
  });

  it('saves the checkbox right away', async () => {
    renderSettings();
    await openSettings();
    fireEvent.click(screen.getByLabelText('Also count In Review'));
    await expectPut({ reviewCap: 2, countInReview: true });
  });

  it.each(['0', '-1', '1.5'])(
    'rejects %s, sends nothing, and stays open on Done',
    async (value) => {
      renderSettings();
      await openSettings();
      const cap = screen.getByLabelText('Review cap');
      fireEvent.change(cap, { target: { value } });
      fireEvent.blur(cap);
      expect(await screen.findByRole('alert')).toHaveTextContent(invalid);
      fireEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(
        screen.getByRole('dialog', { name: 'Board settings' }),
      ).toBeInTheDocument();
      expect(screen.getByRole('alert')).toHaveTextContent(invalid);
      expect(putCalls()).toHaveLength(0);
    },
  );
});
