import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Ticket } from '../board/useTickets';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Textarea } from '../../components/ui/textarea';
import { TicketCommentsTab } from './TicketCommentsTab';
import { useUpdateTicket } from './useTicket';

interface TicketDetailPanelProps {
  ticket: Ticket;
}

export function TicketDetailPanel({ ticket }: TicketDetailPanelProps) {
  const toast = useToast();
  const updateTicket = useUpdateTicket(ticket.id);
  const [editing, setEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [title, setTitle] = useState(ticket.title);
  const [description, setDescription] = useState(ticket.description);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTitle(ticket.title);
    setDescription(ticket.description);
  }, [ticket.title, ticket.description]);

  async function handleSave() {
    setError(null);
    setIsSaving(true);
    try {
      await updateTicket.mutateAsync({
        title: title.trim(),
        description,
      });
      setEditing(false);
      toast.success('Ticket saved');
    } catch {
      setError('Unable to save changes.');
      toast.error('Unable to save ticket');
    } finally {
      setIsSaving(false);
    }
  }

  const saving = isSaving || updateTicket.isPending;

  function handleCancel() {
    setTitle(ticket.title);
    setDescription(ticket.description);
    setEditing(false);
    setError(null);
  }

  return (
    <div className="flex min-h-0 flex-col gap-8">
      <div className="flex flex-col gap-4">
        <div className="flex items-start justify-between gap-4">
          {editing ? (
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="font-display text-lg font-semibold"
            />
          ) : (
            <h3 className="font-display text-lg font-semibold text-text-primary">
              {ticket.title}
            </h3>
          )}

          <div className="flex shrink-0 gap-2">
            {editing ? (
              <>
                <Button
                  type="button"
                  variant="secondary"
                  onClick={handleCancel}
                  disabled={saving}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  onClick={() => void handleSave()}
                  loading={saving}
                  disabled={saving || title.trim().length === 0}
                >
                  {saving ? 'Saving…' : 'Save'}
                </Button>
              </>
            ) : (
              <Button type="button" variant="secondary" onClick={() => setEditing(true)}>
                Edit
              </Button>
            )}
          </div>
        </div>

        {error && (
          <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
            {error}
          </p>
        )}

        {editing ? (
          <Textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Describe the ticket in markdown…"
            className="min-h-[200px] font-mono text-sm leading-relaxed"
          />
        ) : (
          <div className="min-h-[200px] overflow-y-auto rounded-md border border-border bg-surface px-4 py-3 [&_a]:text-accent [&_code]:rounded [&_code]:bg-paper-200 [&_code]:px-1 [&_p+p]:mt-3">
            {description.trim() ? (
              <ReactMarkdown>{description}</ReactMarkdown>
            ) : (
              <p className="font-body text-sm italic text-text-muted">
                No description yet.
              </p>
            )}
          </div>
        )}
      </div>

      <section className="border-t border-border pt-6">
        <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
          Comments
        </h3>
        <TicketCommentsTab ticketId={ticket.id} />
      </section>
    </div>
  );
}
