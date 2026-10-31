import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../lib/api';
import { useBoards } from '../boards/useBoards';
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

const conflict = "This name is too similar to another board's name.";

const board = {
  name: 'Alpha',
  nameLabel: 'Name',
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

let savedBoard = { ...board, slug: 'alpha' };

function PickerNames() {
  const { data } = useBoards();
  return (
    <ul>
      {data?.map((item) => (
        <li key={item.id}>
          {item.name} {item.slug}
        </li>
      ))}
    </ul>
  );
}

function Harness() {
  const capacity = useReviewCapacity('board-1');
  const [open, setOpen] = useState(false);
  return (
    <>
      <PickerNames />
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

function patchCalls() {
  return apiFetch.mock.calls.filter(
    ([url, init]) =>
      url === '/api/boards/board-1' &&
      (init as RequestInit | undefined)?.method === 'PATCH',
  );
}

async function mockApi(url: string, init?: RequestInit) {
  if (url === '/api/boards' && (!init?.method || init.method === 'GET')) {
    return {
      json: async () => [
        {
          id: 'board-1',
          name: savedBoard.name,
          slug: savedBoard.slug,
          createdAt: '2026-01-01T00:00:00Z',
        },
      ],
    };
  }
  if (url === '/api/boards/board-1' && init?.method === 'PATCH') {
    const body = JSON.parse(String(init.body)) as { name: string };
    savedBoard = { ...savedBoard, name: body.name, slug: 'beta' };
    return { json: async () => savedBoard };
  }
  if (init?.method === 'PUT') {
    savedBoard = {
      ...savedBoard,
      ...JSON.parse(String(init.body)),
      waitingOnReview: null,
    };
    return { json: async () => savedBoard };
  }
  return { json: async () => savedBoard };
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
    savedBoard = { ...board, slug: 'alpha' };
    apiFetch.mockReset();
    apiFetch.mockImplementation(mockApi);
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
    expect(screen.getByLabelText('Name')).toHaveFocus();
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

  it('renames the board and shows the new name and slug without a reload', async () => {
    renderSettings();
    expect(await screen.findByText('Alpha alpha')).toBeInTheDocument();
    await openSettings();
    const name = screen.getByLabelText('Name');
    fireEvent.change(name, { target: { value: 'Beta' } });
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => {
      expect(screen.getByText('Beta beta')).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Board settings' }),
      ).not.toBeInTheDocument();
    });
    expect(patchCalls()).toHaveLength(1);
    expect(patchCalls()[0][1]).toEqual(
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ name: 'Beta' }),
      }),
    );
  });

  it('sends one rename PATCH for blur followed by Done', async () => {
    renderSettings();
    await openSettings();
    const name = screen.getByLabelText('Name');
    fireEvent.change(name, { target: { value: 'Beta' } });
    fireEvent.blur(name);
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Board settings' }),
      ).not.toBeInTheDocument();
    });
    expect(patchCalls()).toHaveLength(1);
  });

  it.each(['blur', 'Done', 'Escape'] as const)(
    'keeps the drawer open and shows a 409 on %s',
    async (trigger) => {
      apiFetch.mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === 'PATCH') {
          throw new ApiError(409, JSON.stringify({ message: conflict }));
        }
        return mockApi(url, init);
      });
      renderSettings();
      const dialog = await openSettings();
      const name = screen.getByLabelText('Name');
      fireEvent.change(name, { target: { value: 'Taken' } });
      if (trigger === 'blur') {
        fireEvent.blur(name);
      } else if (trigger === 'Done') {
        fireEvent.click(screen.getByRole('button', { name: 'Done' }));
      } else {
        fireEvent.keyDown(dialog, { key: 'Escape' });
      }
      expect(await screen.findByRole('alert')).toHaveTextContent(conflict);
      expect(
        screen.getByRole('dialog', { name: 'Board settings' }),
      ).toBeInTheDocument();
    },
  );

  it('keeps the drawer open when Done is clicked while a blur rename is in flight', async () => {
    let finish!: (error: unknown) => void;
    apiFetch.mockImplementation(async (url: string, init?: RequestInit) => {
      if (init?.method === 'PATCH') {
        await new Promise<never>((_, reject) => {
          finish = reject;
        });
      }
      return mockApi(url, init);
    });
    renderSettings();
    await openSettings();
    const name = screen.getByLabelText('Name');
    fireEvent.change(name, { target: { value: 'Taken' } });
    fireEvent.blur(name);
    await waitFor(() => {
      expect(patchCalls()).toHaveLength(1);
    });
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    finish(new ApiError(409, JSON.stringify({ message: conflict })));
    expect(await screen.findByRole('alert')).toHaveTextContent(conflict);
    expect(
      screen.getByRole('dialog', { name: 'Board settings' }),
    ).toBeInTheDocument();
    expect(patchCalls()).toHaveLength(1);
  });
});
