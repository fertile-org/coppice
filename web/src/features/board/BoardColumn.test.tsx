import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { BoardColumn } from './BoardColumn';
import { BOARD_COLUMNS } from './columns';

const backlog = BOARD_COLUMNS[0]!;

function renderColumn(
  onQuickAdd: (title: string) => Promise<void>,
  isAdding = false,
) {
  return render(
    <ToastProvider>
      <BoardColumn
        column={backlog}
        tickets={[]}
        hierarchyIndex={new Map()}
        agentsById={new Map()}
        showQuickAdd
        onQuickAdd={onQuickAdd}
        isAdding={isAdding}
        onOpenTicket={() => {}}
      />
    </ToastProvider>,
  );
}

describe('BoardColumn quick-add', () => {
  it('stays enabled and focused while a create is pending', async () => {
    let finish: (value?: void | PromiseLike<void>) => void = () => {};
    const onQuickAdd = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    renderColumn(onQuickAdd, true);

    const input = screen.getByPlaceholderText('Add ticket…');
    input.focus();
    fireEvent.change(input, { target: { value: 'New ticket' } });
    fireEvent.submit(input.closest('form')!);

    expect(onQuickAdd).toHaveBeenCalledWith('New ticket');
    expect(input).toBeEnabled();
    expect(input).toHaveFocus();
    expect(input).toHaveValue('New ticket');

    finish();
    await waitFor(() => expect(input).toHaveValue(''));
    expect(input).toHaveFocus();
  });

  it('creates exactly one ticket when Enter is pressed twice quickly', () => {
    const onQuickAdd = vi.fn(() => new Promise<void>(() => {}));
    renderColumn(onQuickAdd);

    const input = screen.getByPlaceholderText('Add ticket…');
    fireEvent.change(input, { target: { value: 'Once' } });
    const form = input.closest('form')!;
    fireEvent.submit(form);
    fireEvent.submit(form);

    expect(onQuickAdd).toHaveBeenCalledTimes(1);
    expect(onQuickAdd).toHaveBeenCalledWith('Once');
  });

  it('keeps the title and shows an error toast when create fails', async () => {
    const onQuickAdd = vi.fn().mockRejectedValue(new Error('boom'));
    renderColumn(onQuickAdd);

    const input = screen.getByPlaceholderText('Add ticket…');
    fireEvent.change(input, { target: { value: 'Keep me' } });
    fireEvent.submit(input.closest('form')!);

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Unable to create ticket.',
    );
    expect(input).toHaveValue('Keep me');
    expect(input).toBeEnabled();
  });
});
