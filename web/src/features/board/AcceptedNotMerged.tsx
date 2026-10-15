import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useToast } from '../../components/ToastProvider';
import { parseGitConflict, type GitConflict } from '../tickets/conflictOffer';
import {
  MergeBranchDialog,
  MergeBranchTrigger,
} from '../tickets/TicketGitActions';
import { useResolveConflict, useTicketGitInfo } from '../tickets/useTicket';
import { acceptedNotMergedCopy } from './acceptedNotMergedCopy';
import { ticketsQueryKey, type Ticket } from './useTickets';

export function AcceptedNotMerged({ ticket }: { ticket: Ticket }) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const gitInfo = useTicketGitInfo(ticket.id, true);
  const resolveConflict = useResolveConflict(ticket.id);
  const [open, setOpen] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [conflict, setConflict] = useState<GitConflict | null>(null);
  const [asking, setAsking] = useState(false);
  const info = gitInfo.data;

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

  async function handleAsk() {
    if (!conflict?.canAskAssignee || asking) {
      return;
    }
    setAsking(true);
    try {
      await resolveConflict.mutateAsync({
        baseBranch: conflict.baseBranch,
        files: conflict.files,
      });
      setConflict(null);
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
          onClose={() => setOpen(false)}
          ticketId={ticket.id}
          defaultBranch={info.defaultBranch}
          branches={info.branches}
          ticketBranch={info.ticketBranch}
          conflict={conflict}
          asking={asking}
          onConflict={setConflict}
          onAsk={() => void handleAsk()}
          onMerged={handleMerged}
        />
      )}
    </div>
  );
}
