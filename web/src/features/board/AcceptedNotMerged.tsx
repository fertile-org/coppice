import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Button } from '../../components/ui/button';
import { useToast } from '../../components/ToastProvider';
import { parseGitConflict } from '../tickets/conflictOffer';
import { useMergeTicketBranch, useTicketGitInfo } from '../tickets/useTicket';
import { acceptedNotMergedCopy } from './acceptedNotMergedCopy';
import { ticketsQueryKey, type Ticket } from './useTickets';

export function AcceptedNotMerged({ ticket }: { ticket: Ticket }) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const gitInfo = useTicketGitInfo(ticket.id, true);
  const mergeBranch = useMergeTicketBranch(ticket.id);
  const [conflictFiles, setConflictFiles] = useState<string[] | null>(null);
  const [merged, setMerged] = useState(false);
  const baseBranch = gitInfo.data?.defaultBranch ?? '';

  if (merged) {
    return null;
  }

  async function handleMerge() {
    if (!baseBranch || mergeBranch.isPending) {
      return;
    }
    setConflictFiles(null);
    try {
      await mergeBranch.mutateAsync(baseBranch);
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
      toast.success(acceptedNotMergedCopy.success);
      setMerged(true);
    } catch (err) {
      const conflict = parseGitConflict(err);
      if (conflict?.operation === 'merge') {
        setConflictFiles(conflict.files);
      }
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
      <Button
        type="button"
        size="sm"
        variant="secondary"
        disabled={!baseBranch || mergeBranch.isPending}
        onClick={() => void handleMerge()}
      >
        {mergeBranch.isPending
          ? acceptedNotMergedCopy.merging
          : acceptedNotMergedCopy.mergeButton}
      </Button>
      {conflictFiles && (
        <div className="space-y-1">
          <p className="font-body text-xs text-danger">
            {acceptedNotMergedCopy.conflictError}
          </p>
          <ul className="space-y-0.5">
            {conflictFiles.map((file) => (
              <li
                key={file}
                className="truncate font-mono text-xs text-text-secondary"
              >
                {file}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
