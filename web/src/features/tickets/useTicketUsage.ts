import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { ticketUsageSchema, type TicketUsage } from '../../lib/schemas/usage';

export function ticketUsageQueryKey(ticketId: string) {
  return ['ticket-usage', ticketId] as const;
}

async function fetchTicketUsage(ticketId: string): Promise<TicketUsage> {
  const response = await apiFetch(`/api/tickets/${ticketId}/usage`);
  return ticketUsageSchema.parse(await response.json());
}

/** Poll while a run is active so its tokens grow on screen. */
export function useTicketUsage(ticketId: string, refetchInterval: number | false = false) {
  return useQuery({
    queryKey: ticketUsageQueryKey(ticketId),
    queryFn: () => fetchTicketUsage(ticketId),
    refetchInterval,
  });
}
