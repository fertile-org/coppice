import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type {
  PendingRecommendation,
  PendingSplitRecommendation,
} from '../../lib/schemas/ticket';
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
  substatusMetadata?: Record<string, unknown>;
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
  pendingAssignRecommendation?: PendingRecommendation | null;
  parentTicketId?: string | null;
  pendingSplitRecommendation?: PendingSplitRecommendation | null;
  clarificationRound?: number;
  hasActiveRun?: boolean;
  archivedAt?: string | null;
}

/** Prefix for all ticket-list queries for a project (any includeArchived variant). */
export function ticketsQueryKey(projectId: string) {
  return ['tickets', projectId] as const;
}

function ticketsListQueryKey(projectId: string, includeArchived: boolean) {
  return [...ticketsQueryKey(projectId), includeArchived] as const;
}

async function fetchTickets(
  projectId: string,
  includeArchived: boolean,
): Promise<Ticket[]> {
  const params = includeArchived ? '?includeArchived=true' : '';
  const res = await apiFetch(`/api/projects/${projectId}/tickets${params}`);
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

export function useTickets(
  projectId: string | undefined,
  includeArchived = false,
) {
  return useQuery({
    queryKey: ticketsListQueryKey(projectId ?? '', includeArchived),
    queryFn: () => fetchTickets(projectId!, includeArchived),
    enabled: Boolean(projectId),
    refetchInterval: (query) => {
      const tickets = query.state.data;
      if (tickets?.some((ticket) => ticket.hasActiveRun)) {
        return 3000;
      }
      return false;
    },
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

export function useUpdateTicketStatus(
  projectId: string,
  includeArchived = false,
) {
  const queryClient = useQueryClient();
  const listKey = ticketsListQueryKey(projectId, includeArchived);

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
      const previous = queryClient.getQueryData<Ticket[]>(listKey);
      queryClient.setQueryData<Ticket[]>(listKey, (old) =>
        old?.map((ticket) =>
          ticket.id === ticketId ? { ...ticket, status } : ticket,
        ),
      );
      return { previous };
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) {
        queryClient.setQueryData(listKey, context.previous);
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: ticketsQueryKey(projectId),
      });
    },
  });
}
