export type BoardColumnSnapshot = {
  ticketIds: readonly string[];
};

export type BoardFocus =
  | { type: 'card'; ticketId: string }
  | { type: 'heading'; column: number };

export function defaultBoardFocus(
  columns: readonly BoardColumnSnapshot[],
): BoardFocus {
  if (columns[0]?.ticketIds.length) {
    return { type: 'card', ticketId: columns[0].ticketIds[0] };
  }
  return { type: 'heading', column: 0 };
}

export function reconcileBoardFocus(
  columns: readonly BoardColumnSnapshot[],
  focus: BoardFocus | null,
): BoardFocus {
  if (!focus) return defaultBoardFocus(columns);
  if (focus.type === 'heading') {
    const column = columns[focus.column];
    if (!column) return defaultBoardFocus(columns);
    if (column.ticketIds.length === 0) return focus;
    return { type: 'card', ticketId: column.ticketIds[0] };
  }
  if (columns.some((column) => column.ticketIds.includes(focus.ticketId))) {
    return focus;
  }
  return defaultBoardFocus(columns);
}

function locate(
  columns: readonly BoardColumnSnapshot[],
  focus: BoardFocus,
): { column: number; index: number } | null {
  if (focus.type === 'heading') {
    if (focus.column < 0 || focus.column >= columns.length) return null;
    return { column: focus.column, index: -1 };
  }
  for (let column = 0; column < columns.length; column += 1) {
    const index = columns[column].ticketIds.indexOf(focus.ticketId);
    if (index >= 0) return { column, index };
  }
  return null;
}

function focusAt(
  columns: readonly BoardColumnSnapshot[],
  column: number,
  index: number,
): BoardFocus {
  const ids = columns[column]?.ticketIds ?? [];
  if (ids.length === 0) return { type: 'heading', column };
  const clamped = Math.min(Math.max(index, 0), ids.length - 1);
  return { type: 'card', ticketId: ids[clamped] };
}

/** J/K and Up/Down. Stays put on a heading and at either end. */
export function moveCard(
  columns: readonly BoardColumnSnapshot[],
  focus: BoardFocus,
  direction: 1 | -1,
): BoardFocus {
  const located = locate(columns, focus);
  if (!located || located.index < 0) return focus;
  const ids = columns[located.column].ticketIds;
  const next = located.index + direction;
  if (next < 0 || next >= ids.length) return focus;
  return { type: 'card', ticketId: ids[next] };
}

/**
 * H/L and Left/Right. Lands on the same index, the last card, or the heading
 * when the neighbor is empty. Does not wrap.
 */
export function moveColumn(
  columns: readonly BoardColumnSnapshot[],
  focus: BoardFocus,
  direction: 1 | -1,
): BoardFocus {
  const located = locate(columns, focus);
  if (!located) return focus;
  const nextColumn = located.column + direction;
  if (nextColumn < 0 || nextColumn >= columns.length) return focus;
  const index = located.index < 0 ? 0 : located.index;
  return focusAt(columns, nextColumn, index);
}

/** G then 1–9. Focuses the first card, or the heading when the column is empty. */
export function focusColumnStart(
  columns: readonly BoardColumnSnapshot[],
  column: number,
): BoardFocus | null {
  if (column < 0 || column >= columns.length) return null;
  return focusAt(columns, column, 0);
}

export function boardFocusDomId(focus: BoardFocus): string {
  return focus.type === 'card' ? `card:${focus.ticketId}` : `heading:${focus.column}`;
}

export function movedToColumnAnnouncement(columnLabel: string): string {
  return `Moved to ${columnLabel}`;
}
