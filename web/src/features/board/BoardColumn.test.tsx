import '@testing-library/jest-dom/vitest';
import { DndContext } from '@dnd-kit/core';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { ToastProvider } from '../../components/ToastProvider';
import { BoardColumn } from './BoardColumn';
import { BOARD_COLUMNS } from './columns';
import type { Ticket } from './useTickets';

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

const done = BOARD_COLUMNS.find((column) => column.status === 'done')!;
const ready = BOARD_COLUMNS.find((column) => column.status === 'ready')!;

function makeTicket(
  overrides: Pick<Ticket, 'id' | 'title' | 'status'> & Partial<Ticket>,
): Ticket {
  return {
    boardId: 'board-1',
    description: '',
    createdBy: 'user',
    createdAt: '2026-10-15T00:00:00.000Z',
    updatedAt: '2026-10-15T00:00:00.000Z',
    lastActivityAt: '2026-10-15T00:00:00.000Z',
    ...overrides,
  };
}

function renderTickets(
  column: (typeof BOARD_COLUMNS)[number],
  tickets: Ticket[],
  extras: { totalCount?: number; onShowMore?: () => void } = {},
) {
  return render(
    <MemoryRouter>
      <ToastProvider>
        <DndContext>
          <BoardColumn
            column={column}
            tickets={tickets}
            hierarchyIndex={new Map()}
            agentsById={new Map()}
            onOpenTicket={() => {}}
            totalCount={extras.totalCount}
            onShowMore={extras.onShowMore}
          />
        </DndContext>
      </ToastProvider>
    </MemoryRouter>,
  );
}

describe('BoardColumn show more', () => {
  it('shows the total in the header and a count line with Show N more', () => {
    const tickets = Array.from({ length: 10 }, (_, index) =>
      makeTicket({
        id: `done-${index}`,
        title: `Done ${index}`,
        status: 'done',
      }),
    );
    renderTickets(done, tickets, { totalCount: 32, onShowMore: vi.fn() });

    const column = screen.getByRole('region', { name: 'Done' });
    expect(within(column).getByText('32')).toBeVisible();
    expect(within(column).getByText('Showing 10 of 32')).toBeVisible();
    expect(within(column).getByRole('button', { name: 'Show 10 more' })).toBeVisible();
    expect(within(column).getAllByText(/^Done \d+$/)).toHaveLength(10);
  });

  it('adds cards on each click and hides the footer once every ticket is shown', () => {
    const onShowMore = vi.fn();
    const first = Array.from({ length: 10 }, (_, index) =>
      makeTicket({
        id: `done-${index}`,
        title: `Done ${index}`,
        status: 'done',
      }),
    );
    const { rerender } = renderTickets(done, first, {
      totalCount: 22,
      onShowMore,
    });

    fireEvent.click(screen.getByRole('button', { name: 'Show 10 more' }));
    expect(onShowMore).toHaveBeenCalledTimes(1);

    const twenty = Array.from({ length: 20 }, (_, index) =>
      makeTicket({
        id: `done-${index}`,
        title: `Done ${index}`,
        status: 'done',
      }),
    );
    rerender(
      <MemoryRouter>
        <ToastProvider>
          <DndContext>
            <BoardColumn
              column={done}
              tickets={twenty}
              hierarchyIndex={new Map()}
              agentsById={new Map()}
              onOpenTicket={() => {}}
              totalCount={22}
              onShowMore={onShowMore}
            />
          </DndContext>
        </ToastProvider>
      </MemoryRouter>,
    );

    expect(screen.getByText('Showing 20 of 22')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Show 2 more' })).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'Show 2 more' }));
    expect(onShowMore).toHaveBeenCalledTimes(2);

    const all = Array.from({ length: 22 }, (_, index) =>
      makeTicket({
        id: `done-${index}`,
        title: `Done ${index}`,
        status: 'done',
      }),
    );
    rerender(
      <MemoryRouter>
        <ToastProvider>
          <DndContext>
            <BoardColumn
              column={done}
              tickets={all}
              hierarchyIndex={new Map()}
              agentsById={new Map()}
              onOpenTicket={() => {}}
              totalCount={22}
              onShowMore={onShowMore}
            />
          </DndContext>
        </ToastProvider>
      </MemoryRouter>,
    );

    expect(screen.queryByText(/Showing \d+ of \d+/)).toBeNull();
    expect(screen.queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
  });

  it('shows no count line or button when a column has 10 or fewer tickets', () => {
    const tickets = Array.from({ length: 10 }, (_, index) =>
      makeTicket({
        id: `ready-${index}`,
        title: `Ready ${index}`,
        status: 'ready',
      }),
    );
    renderTickets(ready, tickets);

    const column = screen.getByRole('region', { name: 'Ready' });
    expect(within(column).getByText('10')).toBeVisible();
    expect(within(column).queryByText(/Showing \d+ of \d+/)).toBeNull();
    expect(within(column).queryByRole('button', { name: /Show \d+ more/ })).toBeNull();
    expect(within(column).getAllByText(/^Ready \d+$/)).toHaveLength(10);
  });
});
