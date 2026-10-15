import { useContext, useEffect, useRef } from 'react';
import { KeyboardContext } from '../keyboard/keyboardContext';
import {
  boardFocusDomId,
  focusColumnStart,
  moveCard,
  moveColumn,
  type BoardColumnSnapshot,
  type BoardFocus,
} from './boardFocus';

type BoardKeyboardInput = {
  enabled: boolean;
  columnsRef: { current: readonly BoardColumnSnapshot[] };
  focusRef: { current: BoardFocus };
  onMove: (next: BoardFocus) => void;
  onOpenTicket: (ticketId: string) => void;
};

export function useBoardKeyboard({
  enabled,
  columnsRef,
  focusRef,
  onMove,
  onOpenTicket,
}: BoardKeyboardInput) {
  const keyboard = useContext(KeyboardContext);
  const enabledRef = useRef(enabled);
  const onMoveRef = useRef(onMove);
  const onOpenRef = useRef(onOpenTicket);

  useEffect(() => {
    enabledRef.current = enabled;
    onMoveRef.current = onMove;
    onOpenRef.current = onOpenTicket;
  }, [enabled, onMove, onOpenTicket]);

  useEffect(() => {
    if (!keyboard) return;

    const bind = (id: string, run: () => void | boolean) =>
      keyboard.register(id, {
        run,
        enabled: () => enabledRef.current,
      });

    const cleanups = [
      bind('board.nextCard', () => {
        onMoveRef.current(moveCard(columnsRef.current, focusRef.current, 1));
      }),
      bind('board.previousCard', () => {
        onMoveRef.current(moveCard(columnsRef.current, focusRef.current, -1));
      }),
      bind('board.nextColumn', () => {
        onMoveRef.current(moveColumn(columnsRef.current, focusRef.current, 1));
      }),
      bind('board.previousColumn', () => {
        onMoveRef.current(moveColumn(columnsRef.current, focusRef.current, -1));
      }),
      keyboard.register('ticket.open', {
        run: () => {
          const focus = focusRef.current;
          if (focus.type !== 'card') return false;
          const active = document.activeElement;
          if (!(active instanceof HTMLElement)) return false;
          if (active.dataset.boardFocusId !== boardFocusDomId(focus)) return false;
          onOpenRef.current(focus.ticketId);
        },
        // A heading has no ticket to open, so the command stays out of the palette.
        enabled: () => enabledRef.current && focusRef.current.type === 'card',
      }),
      ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) =>
        bind(`board.column.${n}`, () => {
          const next = focusColumnStart(columnsRef.current, n - 1);
          if (next) onMoveRef.current(next);
        }),
      ),
    ];

    return () => {
      for (const cleanup of cleanups) cleanup();
    };
  }, [keyboard, columnsRef, focusRef]);
}
