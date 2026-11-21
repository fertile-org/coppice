import { apiFetch } from '../../lib/api';

/**
 * Same request as the Approve plan button. `skipHumanReview` is sent only
 * when given; leaving it out keeps the ticket's current flag.
 * `acceptRecommended` lets Approve proceed when live sets still use the
 * agent's recommendation (no explicit pick).
 */
export async function approveTicketPlan(
  ticketId: string,
  skipHumanReview?: boolean,
  acceptRecommended?: boolean,
): Promise<unknown> {
  const body: { skipHumanReview?: boolean; acceptRecommended?: boolean } = {};
  if (skipHumanReview !== undefined) body.skipHumanReview = skipHumanReview;
  if (acceptRecommended) body.acceptRecommended = true;
  const res = await apiFetch(`/api/tickets/${ticketId}/approve-plan`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: Object.keys(body).length === 0 ? undefined : JSON.stringify(body),
  });
  return res.json();
}
