import { apiFetch } from '../../lib/api';

/**
 * Same request as the Approve plan button. `skipHumanReview` is sent only
 * when given; leaving it out keeps the ticket's current flag.
 */
export async function approveTicketPlan(
  ticketId: string,
  skipHumanReview?: boolean,
): Promise<unknown> {
  const res = await apiFetch(`/api/tickets/${ticketId}/approve-plan`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: skipHumanReview === undefined ? undefined : JSON.stringify({ skipHumanReview }),
  });
  return res.json();
}
