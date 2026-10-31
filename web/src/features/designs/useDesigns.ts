import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { designSetListSchema, type DesignSet } from '../../lib/schemas/design';
import { agentRunsQueryKey } from '../tickets/useAgentRuns';
import { ticketQueryKey } from '../tickets/useTicket';

export type DesignScope = string | { chatSessionId: string };

/** The set the Detail card opens: the plan's, else the newest live one. */
export function planCardDesign(sets: DesignSet[]): DesignSet | undefined {
  const live = sets.filter((set) => !set.superseded);
  return live.find((set) => set.fromPlanRun) ?? live[0];
}

/**
 * Whether work follows this set: a plan-run set until someone picks on a work-run
 * set, and any other set only once picked. Mirrors the server's effective design.
 */
export function designDrivesWork(design: DesignSet, sets: DesignSet[]): boolean {
  if (!design.fromPlanRun) return design.chosen !== null;
  return !sets.some((set) => !set.superseded && !set.fromPlanRun && set.chosen !== null);
}

export function designsQueryKey(scope: DesignScope) {
  return typeof scope === 'string'
    ? (['designs', 'ticket', scope] as const)
    : (['designs', 'chat', scope.chatSessionId] as const);
}

function designsPath(scope: DesignScope): string {
  return typeof scope === 'string'
    ? `/api/tickets/${encodeURIComponent(scope)}/designs`
    : `/api/chat/sessions/${encodeURIComponent(scope.chatSessionId)}/designs`;
}

async function fetchDesigns(scope: DesignScope): Promise<DesignSet[]> {
  const res = await apiFetch(designsPath(scope));
  return designSetListSchema.parse(await res.json());
}

function scopeEnabled(scope: DesignScope | undefined): scope is DesignScope {
  if (typeof scope === 'string') return scope.length > 0;
  return Boolean(scope?.chatSessionId);
}

export function useDesigns(scope?: DesignScope) {
  return useQuery({
    queryKey: scopeEnabled(scope) ? designsQueryKey(scope) : ['designs', 'idle'],
    queryFn: () => {
      if (!scopeEnabled(scope)) throw new Error('missing design scope');
      return fetchDesigns(scope);
    },
    enabled: scopeEnabled(scope),
    retry: false,
    staleTime: 0,
    refetchOnMount: 'always',
  });
}

export function useChooseDesign(scope?: DesignScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ setId, optionId }: { setId: string; optionId: string }) => {
      await apiFetch(`/api/designs/${encodeURIComponent(setId)}/choose`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ optionId }),
      });
    },
    // A pick can record a plan change request and start a plan run. A refused pick
    // means the cached footer was out of date, so it refreshes too.
    onSettled: () => {
      if (scopeEnabled(scope)) {
        void queryClient.invalidateQueries({ queryKey: designsQueryKey(scope) });
        if (typeof scope === 'string') {
          void queryClient.invalidateQueries({ queryKey: ticketQueryKey(scope) });
          void queryClient.invalidateQueries({ queryKey: agentRunsQueryKey(scope) });
        }
      }
    },
  });
}
