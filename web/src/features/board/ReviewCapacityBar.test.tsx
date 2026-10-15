import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ReviewCapacityBar } from './ReviewCapacityBar';

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
  waitingOnReview: '2 waiting on your review',
  reviewCapLabel: 'Review cap',
  reviewCapHint:
    'How many tickets can wait for your review at once. Leave it empty for no cap.',
  countInReviewLabel: 'Also count In Review',
  reviewCapInvalid: invalid,
};

function renderBar() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ReviewCapacityBar boardId="board-1" />
    </QueryClientProvider>,
  );
}

describe('ReviewCapacityBar', () => {
  beforeEach(() => {
    apiFetch.mockReset();
    apiFetch.mockResolvedValue({
      json: async () => board,
    });
  });

  it('shows the server waiting line, label, and hint', async () => {
    renderBar();
    expect(await screen.findByText('2 waiting on your review')).toBeInTheDocument();
    expect(screen.getByLabelText('Review cap')).toHaveValue(2);
    expect(screen.getByLabelText('Also count In Review')).not.toBeChecked();
    expect(
      screen.getByText(
        'How many tickets can wait for your review at once. Leave it empty for no cap.',
      ),
    ).toBeInTheDocument();
  });

  it('saves a cap without inventing its own wording', async () => {
    apiFetch.mockResolvedValueOnce({ json: async () => board }).mockResolvedValueOnce({
      json: async () => ({ ...board, reviewCap: 4, waitingOnReview: null }),
    });
    renderBar();
    const cap = await screen.findByLabelText('Review cap');
    fireEvent.change(cap, { target: { value: '4' } });
    fireEvent.blur(cap);
    await waitFor(() => {
      expect(apiFetch).toHaveBeenCalledWith(
        '/api/boards/board-1/review-capacity',
        expect.objectContaining({
          method: 'PUT',
          body: JSON.stringify({
            reviewCap: 4,
            countInReview: false,
          }),
        }),
      );
    });
    await waitFor(() => {
      expect(screen.queryByText('2 waiting on your review')).not.toBeInTheDocument();
    });
  });

  it('rejects zero in the field and does not save it', async () => {
    renderBar();
    const cap = await screen.findByLabelText('Review cap');
    fireEvent.change(cap, { target: { value: '0' } });
    fireEvent.blur(cap);
    expect(await screen.findByRole('alert')).toHaveTextContent(invalid);
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
});
