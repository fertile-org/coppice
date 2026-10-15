import { useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { useFinalApprove } from './useTicket';

export const ACCEPT_COPY = {
  title: 'Accept this ticket?',
  body: 'This accepts the work currently in Wait for Human Review.',
  cancel: 'Cancel',
  confirm: 'Accept',
} as const;

export function AcceptTicketDialog({
  ticketId,
  open,
  onClose,
}: {
  ticketId: string;
  open: boolean;
  onClose: () => void;
}) {
  const approve = useFinalApprove(ticketId);
  const [error, setError] = useState<string | null>(null);

  async function confirm() {
    setError(null);
    try {
      await approve.mutateAsync();
      onClose();
    } catch {
      setError('Unable to accept ticket.');
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !approve.isPending) onClose();
      }}
    >
      <DialogContent
        className="z-[60] max-w-md"
        overlayClassName="z-[60]"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          const panel = event.currentTarget;
          if (panel instanceof HTMLElement) {
            panel.querySelector<HTMLElement>('[data-shortcut-cancel]')?.focus();
          }
        }}
      >
        <DialogTitle>{ACCEPT_COPY.title}</DialogTitle>
        <DialogDescription className="mt-1">{ACCEPT_COPY.body}</DialogDescription>
        {error && (
          <p className="mt-3 font-body text-sm text-danger">{error}</p>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button
            type="button"
            variant="secondary"
            data-shortcut-cancel=""
            onClick={onClose}
            disabled={approve.isPending}
          >
            {ACCEPT_COPY.cancel}
          </Button>
          <Button type="button" onClick={() => void confirm()} disabled={approve.isPending}>
            {approve.isPending ? 'Accepting…' : ACCEPT_COPY.confirm}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
