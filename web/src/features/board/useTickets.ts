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

export interface HumanReview {
  headSha: string;
  shortSha: string;
  stale: boolean;
  /** True after the accepted commit has been merged. */
  merged?: boolean;
  commentIds: string[];
  runIds: string[];
}

export interface ReviewerBrief {
  howItWorks?: string | null;
  changesFromPlan?: string | null;
  lookHereFirst?: string | null;
  commitSha?: string | null;
}

export type CriterionStatus = 'met' | 'not_met' | 'not_checked';

export interface PlanCriterion {
  text: string;
  status: CriterionStatus;
  evidence?: string | null;
}

export interface ApprovedPlan {
  commentId: string;
  markdown: string;
  steps: string[];
  /** Acceptance criteria with the result for the reviewed commit. */
  criteria?: PlanCriterion[];
}

export interface Ticket {
  id: string;
  boardId: string;
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
  skipPlanning?: boolean;
  planApproved?: boolean;
  latestPlanRunStatus?: string | null;
  approvedPlan?: ApprovedPlan | null;
  archivedAt?: string | null;
  attachmentIds?: string[];
  attachments?: {
    id: string;
    filename: string;
    contentType: string;
    sizeBytes: number;
  }[];
  humanReview?: HumanReview | null;
  reviewerBrief?: ReviewerBrief | null;
}

/** Prefix for all ticket-list queries for a board (any includeArchived variant). */
export function ticketsQueryKey(boardId: string) {
  return ['tickets', boardId] as const;
}

function ticketsListQueryKey(boardId: string, includeArchived: boolean) {
  return [...ticketsQueryKey(boardId), includeArchived] as const;
}

async function fetchTickets(
  boardId: string,
  includeArchived: boolean,
): Promise<Ticket[]> {
  const params = includeArchived ? '?includeArchived=true' : '';
  const res = await apiFetch(`/api/boards/${boardId}/tickets${params}`);
  return res.json() as Promise<Ticket[]>;
}

async function createTicket(
  boardId: string,
  title: string,
): Promise<Ticket> {
  const res = await apiFetch(`/api/boards/${boardId}/tickets`, {
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
  boardId: string | undefined,
  includeArchived = false,
) {
  return useQuery({
    queryKey: ticketsListQueryKey(boardId ?? '', includeArchived),
    queryFn: () => fetchTickets(boardId!, includeArchived),
    enabled: Boolean(boardId),
    refetchInterval: (query) => {
      const tickets = query.state.data;
      if (tickets?.some((ticket) => ticket.hasActiveRun)) {
        return 3000;
      }
      return false;
    },
  });
}

export function useCreateTicket(boardId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (title: string) => createTicket(boardId, title),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ticketsQueryKey(boardId),
      });
    },
  });
}

export function useStartPlanRun(boardId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (ticketId: string) => {
      const res = await apiFetch(`/api/tickets/${ticketId}/plan-runs`, {
        method: 'POST',
      });
      return res.json() as Promise<Ticket>;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ticketsQueryKey(boardId),
      });
    },
  });
}

export function useUpdateTicketStatus(
  boardId: string,
  includeArchived = false,
) {
  const queryClient = useQueryClient();
  const listKey = ticketsListQueryKey(boardId, includeArchived);

  return useMutation({
    mutationFn: ({
      ticketId,
      status,
    }: {
      ticketId: string;
      status: TicketStatus;
    }) => patchTicketStatus(ticketId, status),
    onMutate: async ({ ticketId, status }) => {
      await queryClient.cancelQueries({ queryKey: ticketsQueryKey(boardId) });
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
        queryKey: ticketsQueryKey(boardId),
      });
    },
  });
}
