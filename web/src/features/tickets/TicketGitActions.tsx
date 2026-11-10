import type { Ticket } from '../board/useTickets';
import { GitStatusRow } from './GitStatusRow';

export function TicketGitActions({ ticket }: { ticket: Ticket }) {
  if (!ticket.repoId) {
    return null;
  }
  return (
    <div className="space-y-2 rounded-md border border-border bg-surface px-3 py-3">
      <p className="font-body text-xs font-medium text-text-muted">Git</p>
      {ticket.archivedAt && (
        <p className="font-body text-xs text-text-secondary">
          Unarchive to run git actions.
        </p>
      )}
      <GitStatusRow ticket={ticket} variant="panel" />
    </div>
  );
}
