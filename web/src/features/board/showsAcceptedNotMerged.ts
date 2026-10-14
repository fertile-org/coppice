import type { Ticket } from './useTickets';

export function showsAcceptedNotMerged(ticket: Ticket): boolean {
  const review = ticket.humanReview;
  return (
    ticket.status === 'done' &&
    !ticket.archivedAt &&
    Boolean(ticket.repoId) &&
    review != null &&
    review.stale === false &&
    review.merged !== true
  );
}
