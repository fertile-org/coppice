import { useEffect, useRef } from 'react';
import { useRegisterShortcut } from '../keyboard/useKeyboard';
import { requestAssigneeOpen } from '../tickets/assigneeShortcut';
import type { BoardColumnSnapshot, BoardFocus } from './boardFocus';
import type { TicketStatus } from './columns';
import {
  acceptShortcutEnabled,
  addTicketField,
  approveShortcutEnabled,
  codeReviewEnabled,
  columnIndexForFocus,
  moveTargetStatus,
} from './ticketShortcutRules';
import type { Ticket } from './useTickets';

type TicketShortcutsInput = {
  drawerOpen: boolean;
  selectedTicketId: string | null;
  ticketsById: ReadonlyMap<string, Ticket>;
  columnsRef: { current: readonly BoardColumnSnapshot[] };
  focusRef: { current: BoardFocus };
  repos: readonly { id: string; verificationStatus: string }[] | undefined;
  onOpenTicket: (ticketId: string) => void;
  onMoveStatus: (ticketId: string, status: TicketStatus) => void;
  onApprove: (ticketId: string) => void;
  onAccept: (ticket: Ticket) => void;
  onOpenCode: (ticket: Ticket) => void;
};

export function useTicketShortcuts({
  drawerOpen,
  selectedTicketId,
  ticketsById,
  columnsRef,
  focusRef,
  repos,
  onOpenTicket,
  onMoveStatus,
  onApprove,
  onAccept,
  onOpenCode,
}: TicketShortcutsInput) {
  const drawerRef = useRef(drawerOpen);
  const selectedRef = useRef(selectedTicketId);
  const ticketsRef = useRef(ticketsById);
  const reposRef = useRef(repos);
  const openTicketRef = useRef(onOpenTicket);
  const moveRef = useRef(onMoveStatus);
  const approveRef = useRef(onApprove);
  const acceptRef = useRef(onAccept);
  const codeRef = useRef(onOpenCode);

  useEffect(() => {
    drawerRef.current = drawerOpen;
    selectedRef.current = selectedTicketId;
    ticketsRef.current = ticketsById;
    reposRef.current = repos;
    openTicketRef.current = onOpenTicket;
    moveRef.current = onMoveStatus;
    approveRef.current = onApprove;
    acceptRef.current = onAccept;
    codeRef.current = onOpenCode;
  });

  function focusedCard(): Ticket | null {
    const focus = focusRef.current;
    if (focus.type !== 'card') return null;
    return ticketsRef.current.get(focus.ticketId) ?? null;
  }

  function moveTicket(): Ticket | null {
    if (drawerRef.current) {
      const id = selectedRef.current;
      return id ? (ticketsRef.current.get(id) ?? null) : null;
    }
    return focusedCard();
  }

  useRegisterShortcut(
    'ticket.create',
    () => {
      if (drawerRef.current) return false;
      const index = columnIndexForFocus(columnsRef.current, focusRef.current);
      if (index == null) return false;
      const field = addTicketField(index);
      if (!field) return false;
      field.focus();
    },
    () => {
      if (drawerRef.current) return false;
      const index = columnIndexForFocus(columnsRef.current, focusRef.current);
      if (index == null) return false;
      return addTicketField(index) != null;
    },
  );

  useRegisterShortcut(
    'ticket.assign',
    () => {
      if (drawerRef.current) return false;
      const ticket = focusedCard();
      if (!ticket || ticket.archivedAt) return false;
      openTicketRef.current(ticket.id);
      requestAssigneeOpen(ticket.id);
    },
    () => {
      if (drawerRef.current) return false;
      const ticket = focusedCard();
      return Boolean(ticket && !ticket.archivedAt);
    },
  );

  useRegisterShortcut(
    'ticket.moveNext',
    () => {
      const ticket = moveTicket();
      const status = moveTargetStatus(ticket, 1);
      if (!ticket || !status) return false;
      moveRef.current(ticket.id, status);
    },
    () => moveTargetStatus(moveTicket(), 1) != null,
  );

  useRegisterShortcut(
    'ticket.movePrevious',
    () => {
      const ticket = moveTicket();
      const status = moveTargetStatus(ticket, -1);
      if (!ticket || !status) return false;
      moveRef.current(ticket.id, status);
    },
    () => moveTargetStatus(moveTicket(), -1) != null,
  );

  useRegisterShortcut(
    'review.approvePlan',
    () => {
      if (drawerRef.current) return false;
      const ticket = focusedCard();
      if (!approveShortcutEnabled(ticket) || !ticket) return false;
      approveRef.current(ticket.id);
    },
    () => !drawerRef.current && approveShortcutEnabled(focusedCard()),
  );

  useRegisterShortcut(
    'review.accept',
    () => {
      if (drawerRef.current) return false;
      const ticket = focusedCard();
      if (!ticket || !acceptShortcutEnabled(ticket)) return false;
      acceptRef.current(ticket);
    },
    () => !drawerRef.current && acceptShortcutEnabled(focusedCard()),
  );

  useRegisterShortcut(
    'review.openCode',
    () => {
      if (drawerRef.current) return false;
      const ticket = focusedCard();
      if (!ticket?.repoId || !codeReviewEnabled(ticket, reposRef.current)) return false;
      codeRef.current(ticket);
    },
    () => !drawerRef.current && codeReviewEnabled(focusedCard(), reposRef.current),
  );
}
