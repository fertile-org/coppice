import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import { designSetListSchema, type DesignSet } from '../../lib/schemas/design';

export type DesignScope = string | { chatSessionId: string };

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
    onSuccess: () => {
      if (scopeEnabled(scope)) {
        void queryClient.invalidateQueries({ queryKey: designsQueryKey(scope) });
      }
    },
  });
}
