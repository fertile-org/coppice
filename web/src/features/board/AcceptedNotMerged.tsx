import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useRegisterShortcut } from '../keyboard/useKeyboard';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import { selectedConflictAgent, type GitConflict } from '../tickets/conflictOffer';
import {
  MergeBranchDialog,
  MergeBranchTrigger,
} from '../tickets/TicketGitActions';
import { useResolveConflict, useTicketGitInfo } from '../tickets/useTicket';
import { acceptedNotMergedCopy } from './acceptedNotMergedCopy';
import { ticketsQueryKey, type Ticket } from './useTickets';

export function AcceptedNotMerged({
  ticket,
  shortcutActive = false,
}: {
  ticket: Ticket;
  /** True when this card is the board tab stop, so Merge… follows the focused card. */
  shortcutActive?: boolean;
}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const gitInfo = useTicketGitInfo(ticket.id, true);
  const resolveConflict = useResolveConflict(ticket.id);
  const [open, setOpen] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [conflict, setConflict] = useState<GitConflict | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const info = gitInfo.data;

  useRegisterShortcut(
    'review.merge',
    () => setOpen(true),
    () =>
      shortcutActive &&
      Boolean(info) &&
      !hidden &&
      document.querySelector('[data-shortcut-layer="ticket"]') == null,
  );

  if (hidden) {
    return null;
  }

  function handleMerged() {
    queryClient.setQueriesData<Ticket[]>(
      { queryKey: ticketsQueryKey(ticket.boardId) },
      (current) =>
        current?.map((item) =>
          item.id === ticket.id && item.humanReview
            ? {
                ...item,
                humanReview: { ...item.humanReview, merged: true },
              }
            : item,
        ),
    );
    setHidden(true);
  }

  function rememberConflict(next: GitConflict | null) {
    setConflict(next);
    setSelectedAgentId(next?.defaultAgentId ?? next?.agents[0]?.id ?? null);
  }

  function closeDialog() {
    setOpen(false);
  }

  async function handleAsk() {
    const selected = conflict ? selectedConflictAgent(conflict, selectedAgentId) : null;
    if (!conflict || !selected || selected.unavailableReason || asking) {
      return;
    }
    setAsking(true);
    try {
      await resolveConflict.mutateAsync({
        baseBranch: conflict.baseBranch,
        files: conflict.files,
        operation: conflict.operation,
        agentId: selected.id,
      });
      toast.success(`${selected.name} is resolving the conflict`);
      setConflict(null);
      setSelectedAgentId(null);
      setOpen(false);
    } catch (err) {
      const message = parseApiErrorMessage(err);
      toast.error(apiErrorToastMessage(message));
    } finally {
      setAsking(false);
    }
  }

  return (
    <div
      className="mt-2 space-y-1.5"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <p className="font-body text-xs text-text-secondary">
        {acceptedNotMergedCopy.statusLabel}
      </p>
      <MergeBranchTrigger
        size="sm"
        disabled={!info}
        onClick={() => setOpen(true)}
      />
      {info && (
        <MergeBranchDialog
          open={open}
          onClose={closeDialog}
          ticketId={ticket.id}
          defaultBranch={info.defaultBranch}
          branches={info.branches}
          ticketBranch={info.ticketBranch}
          conflict={conflict}
          selectedAgentId={selectedAgentId}
          onSelectAgent={setSelectedAgentId}
          asking={asking}
          onConflict={rememberConflict}
          onAsk={() => void handleAsk()}
          onMerged={handleMerged}
        />
      )}
    </div>
  );
}
