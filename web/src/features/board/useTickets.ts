import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type { TicketStatus } from './columns';

export interface SubstatusDisplay {
  label: string;
  detail?: string;
}

export interface Ticket {
  id: string;
  projectId: string;
  repoId?: string;
  title: string;
  description: string;
  status: TicketStatus;
  substatus?: string;
  priority?: string;
  assigneeAgentId?: string;
  ownerUserId?: string;
  branchName?: string;
  createdBy: string;
  createdById?: string;
  createdAt: string;
  updatedAt: string;
  lastActivityAt: string;
  substatusDisplay?: SubstatusDisplay;
}

export function ticketsQueryKey(projectId: string) {
  return ['tickets', projectId] as const;
}

async function fetchTickets(projectId: string): Promise<Ticket[]> {
  const res = await apiFetch(`/api/projects/${projectId}/tickets`);
  return res.json() as Promise<Ticket[]>;
}

async function createTicket(
  projectId: string,
  title: string,
): Promise<Ticket> {
  const res = await apiFetch(`/api/projects/${projectId}/tickets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title }),
  });
  return res.json() as Promise<Ticket>;
}

async function patchTicketStatus(
  ticketId: string,
  status: TicketStatus,
): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/status`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ status }),
  });
  return res.json() as Promise<Ticket>;
}

export function useTickets(projectId: string | undefined) {
  return useQuery({
    queryKey: ticketsQueryKey(projectId ?? ''),
    queryFn: () => fetchTickets(projectId!),
    enabled: Boolean(projectId),
  });
}

export function useCreateTicket(projectId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (title: string) => createTicket(projectId, title),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ticketsQueryKey(projectId),
      });
    },
  });
}

export function useUpdateTicketStatus(projectId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      ticketId,
      status,
    }: {
      ticketId: string;
      status: TicketStatus;
    }) => patchTicketStatus(ticketId, status),
    onMutate: async ({ ticketId, status }) => {
      await queryClient.cancelQueries({ queryKey: ticketsQueryKey(projectId) });
      const previous = queryClient.getQueryData<Ticket[]>(
        ticketsQueryKey(projectId),
      );
      queryClient.setQueryData<Ticket[]>(ticketsQueryKey(projectId), (old) =>
        old?.map((ticket) =>
          ticket.id === ticketId ? { ...ticket, status } : ticket,
        ),
      );
      return { previous };
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          ticketsQueryKey(projectId),
          context.previous,
        );
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: ticketsQueryKey(projectId),
      });
    },
  });
}
