import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { useAgents } from '../agents/useAgents';
import { ticketsQueryKey } from '../board/useTickets';
import { commentsQueryKey, ticketQueryKey } from '../tickets/useTicket';
import { PLAN_COPY } from './copy';

async function postVerify(ticketId: string, agentId: string) {
  const res = await apiFetch(`/api/tickets/${ticketId}/verify-parent`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(agentId ? { agentId } : {}),
  });
  return res.json();
}

/** Shown on a split parent once every sub-ticket is Done and merged. */
export function VerifyParentAction({ ticketId, boardId }: { ticketId: string; boardId: string }) {
  const queryClient = useQueryClient();
  const agents = useAgents();
  const [agentId, setAgentId] = useState('');
  const [error, setError] = useState<string | null>(null);
  const verify = useMutation({
    mutationFn: () => postVerify(ticketId, agentId),
    onSuccess: () => {
      setError(null);
      void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
      void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
      void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });
  const enabledAgents = (agents.data ?? []).filter((agent) => agent.enabled);

  return (
    <section className="space-y-3 border-t border-border pt-6">
      <div className="flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1 font-body text-xs text-text-muted">
          {PLAN_COPY.verifyAgent}
          <select
            value={agentId}
            onChange={(event) => setAgentId(event.target.value)}
            disabled={verify.isPending}
            className="rounded-md border border-border bg-surface px-2 py-1.5 font-body text-sm text-text-primary"
          >
            <option value="">{PLAN_COPY.verifyDefaultAgent}</option>
            {enabledAgents.map((agent) => (
              <option key={agent.id} value={agent.id}>
                {agent.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          onClick={() => verify.mutate()}
          disabled={verify.isPending}
          className="rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-accent-foreground transition-colors duration-fast hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {verify.isPending ? PLAN_COPY.verifying : PLAN_COPY.verifyParent}
        </button>
      </div>
      {error && <p className="font-body text-sm text-danger">{error}</p>}
    </section>
  );
}
