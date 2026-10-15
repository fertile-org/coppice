import { describe, expect, it } from 'vitest';
import {
  defaultBoardFocus,
  focusColumnStart,
  moveCard,
  moveColumn,
  movedToColumnAnnouncement,
  reconcileBoardFocus,
  type BoardColumnSnapshot,
} from './boardFocus';

function columns(ids: string[][]): BoardColumnSnapshot[] {
  return ids.map((ticketIds) => ({ ticketIds }));
}

describe('board focus', () => {
  const board = columns([
    ['a0', 'a1'],
    [],
    ['c0'],
    [],
    [],
    [],
    [],
    [],
    [],
  ]);

  it('starts on the first Backlog card and does not wrap card or column movement', () => {
    const start = defaultBoardFocus(board);
    expect(start).toEqual({ type: 'card', ticketId: 'a0' });
    expect(moveCard(board, start, -1)).toEqual(start);
    const next = moveCard(board, start, 1);
    expect(next).toEqual({ type: 'card', ticketId: 'a1' });
    expect(moveCard(board, next, 1)).toEqual(next);

    expect(moveColumn(board, start, -1)).toEqual(start);
    expect(moveColumn(board, next, 1)).toEqual({ type: 'heading', column: 1 });
    expect(moveColumn(board, { type: 'card', ticketId: 'c0' }, 1)).toEqual({
      type: 'heading',
      column: 3,
    });
    expect(moveColumn(board, { type: 'heading', column: 8 }, 1)).toEqual({
      type: 'heading',
      column: 8,
    });
  });

  it('keeps the same index, or the last card, and jumps to a column heading', () => {
    const paired = columns([
      ['a0', 'a1', 'a2'],
      ['b0'],
    ]);
    expect(moveColumn(paired, { type: 'card', ticketId: 'a0' }, 1)).toEqual({
      type: 'card',
      ticketId: 'b0',
    });
    expect(moveColumn(paired, { type: 'card', ticketId: 'a2' }, 1)).toEqual({
      type: 'card',
      ticketId: 'b0',
    });
    const fromHeading = moveColumn(board, { type: 'heading', column: 1 }, 1);
    expect(fromHeading).toEqual({ type: 'card', ticketId: 'c0' });
    expect(focusColumnStart(board, 0)).toEqual({ type: 'card', ticketId: 'a0' });
    expect(focusColumnStart(board, 2)).toEqual({ type: 'card', ticketId: 'c0' });
    expect(focusColumnStart(board, 8)).toEqual({ type: 'heading', column: 8 });
    expect(focusColumnStart(board, 9)).toBeNull();
  });

  it('drops a focus whose ticket is gone and announces a move with the column name', () => {
    expect(
      reconcileBoardFocus(board, { type: 'card', ticketId: 'missing' }),
    ).toEqual({ type: 'card', ticketId: 'a0' });
    expect(reconcileBoardFocus(columns([[]]), { type: 'heading', column: 0 })).toEqual({
      type: 'heading',
      column: 0,
    });
    expect(movedToColumnAnnouncement('In Review')).toBe('Moved to In Review');
  });
});
