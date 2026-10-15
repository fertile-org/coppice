import { apiFetch } from '../../lib/api';

/** Same request as the Approve plan button. */
export async function approveTicketPlan(ticketId: string): Promise<unknown> {
  const res = await apiFetch(`/api/tickets/${ticketId}/approve-plan`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
  });
  return res.json();
}
