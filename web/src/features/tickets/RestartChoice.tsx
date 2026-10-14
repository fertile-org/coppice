import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch, parseApiErrorMessage } from '../../lib/api';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '../../components/ui/dialog';
import { ticketsQueryKey } from '../board/useTickets';
import { commentsQueryKey, ticketQueryKey } from './useTicket';

interface RestartPayload {
  pending: boolean;
  resumeLabel?: string;
  otherLabel?: string;
  confirm?: string | null;
}

async function readRestart(ticketId: string): Promise<RestartPayload> {
  const res = await apiFetch(`/api/tickets/${ticketId}/restart`);
  return res.json() as Promise<RestartPayload>;
}

export function RestartChoice({ ticketId, boardId }: { ticketId: string; boardId: string }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const query = useQuery({
    queryKey: ['ticket-restart', ticketId],
    queryFn: () => readRestart(ticketId),
  });

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ['ticket-restart', ticketId] });
    void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
    void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
  }

  const act = useMutation({
    mutationFn: async (action: 'resume' | 'discard' | 'cancel') => {
      await apiFetch(`/api/tickets/${ticketId}/restart/${action}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{}',
      });
    },
    onSuccess: () => {
      setError(null);
      setConfirming(false);
      refresh();
    },
    onError: (err: unknown) => setError(parseApiErrorMessage(err)),
  });

  const choice = query.data;
  if (!choice?.pending || !choice.resumeLabel || !choice.otherLabel) {
    return null;
  }

  const otherIsDiscard = Boolean(choice.confirm);

  return (
    <section className="border-t border-border pt-6">
      <div className="flex flex-wrap gap-2">
        <Button type="button" disabled={act.isPending} onClick={() => act.mutate('resume')}>
          {choice.resumeLabel}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={act.isPending}
          onClick={() => {
            if (otherIsDiscard) {
              setConfirming(true);
            } else {
              act.mutate('cancel');
            }
          }}
        >
          {choice.otherLabel}
        </Button>
      </div>
      {error && <p className="mt-2 font-body text-sm text-danger">{error}</p>}
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent
          className="z-[60] max-w-md"
          overlayClassName="z-[60]"
          aria-describedby={undefined}
        >
          <DialogTitle className="font-body text-base font-medium">{choice.confirm}</DialogTitle>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={act.isPending}
              onClick={() => act.mutate('discard')}
            >
              {choice.otherLabel}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </section>
  );
}
