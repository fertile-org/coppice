import type { Ticket } from '../board/useTickets';

export type PlanCardKind = 'writing' | 'answers' | 'waiting' | 'needs-run' | 'none';

/** Card label for the planning columns. Skip planning shows none of these. */
export function planCardState(ticket: Pick<
  Ticket,
  'status' | 'skipPlanning' | 'planApproved' | 'latestPlanRunStatus' | 'planQuestions'
>): PlanCardKind {
  if (ticket.skipPlanning) return 'none';
  const run = ticket.latestPlanRunStatus ?? null;
  if (run === 'queued' || run === 'running') return 'writing';
  if (ticket.status === 'plan_review' && ticket.planQuestions?.waiting) return 'answers';
  if (ticket.status === 'plan_review') return 'waiting';
  const needsPlan =
    ticket.status === 'backlog' ||
    (ticket.status === 'ready' && !ticket.planApproved);
  if (!needsPlan) return 'none';
  return 'needs-run';
}
