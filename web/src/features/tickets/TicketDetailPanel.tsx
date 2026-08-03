import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { GitBranch } from 'lucide-react';
import type { TicketParentSummary } from '../board/ticketHierarchy';
import type { Ticket } from '../board/useTickets';
import { TicketMarkdown } from '../../components/TicketMarkdown';
import { useToast } from '../../components/ToastProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Textarea } from '../../components/ui/textarea';
import { TicketStatusBadge } from './TicketStatusBadge';
import { TicketCommentsTab } from './TicketCommentsTab';
import { useTicketChildren, useUpdateTicket } from './useTicket';

interface TicketDetailPanelProps {
  ticket: Ticket;
  parentTicket?: TicketParentSummary | null;
}

export function TicketDetailPanel({
  ticket,
  parentTicket = null,
}: TicketDetailPanelProps) {
  const toast = useToast();
  const [, setSearchParams] = useSearchParams();
  const updateTicket = useUpdateTicket(ticket.id);
  const { data: children } = useTicketChildren(ticket.id);
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
          <div className="min-h-[200px] overflow-y-auto rounded-md border border-border bg-surface px-4 py-3">
            {description.trim() ? (
              <TicketMarkdown>{description}</TicketMarkdown>
            ) : (
              <p className="font-body text-sm italic text-text-muted">
                No description yet.
              </p>
            )}
          </div>
        )}
      </div>

      {parentTicket && ticket.parentTicketId === parentTicket.id && (
        <section className="border-t border-border pt-6">
          <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
            Parent ticket
          </h3>
          <button
            type="button"
            aria-label={`Open parent ticket: ${parentTicket.title}`}
            onClick={() => setSearchParams({ ticket: parentTicket.id })}
            className="flex min-h-11 w-full min-w-0 items-center gap-2 rounded-md border border-border bg-surface px-3 py-2 text-left transition-colors duration-fast hover:bg-paper-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
          >
            <GitBranch
              className="h-4 w-4 shrink-0 text-text-secondary"
              aria-hidden="true"
            />
            <span
              className="min-w-0 truncate font-body text-sm font-medium text-text-primary"
              title={parentTicket.title}
            >
              {parentTicket.title}
            </span>
          </button>
        </section>
      )}

      {children && children.length > 0 && (
        <section className="border-t border-border pt-6">
          <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
            Child tickets
          </h3>
          <ul className="space-y-2">
            {children.map((child) => (
              <li key={child.id}>
                <button
                  type="button"
                  onClick={() => setSearchParams({ ticket: child.id })}
                  className="flex min-h-11 w-full items-center justify-between gap-3 rounded-md border border-border bg-surface px-3 py-2 text-left transition-colors duration-fast hover:bg-paper-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
                >
                  <span className="truncate font-body text-sm font-medium text-text-primary">
                    {child.title}
                  </span>
                  <TicketStatusBadge status={child.status} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="border-t border-border pt-6">
        <h3 className="mb-4 font-display text-base font-semibold tracking-tight text-text-primary">
          Comments
        </h3>
        <TicketCommentsTab ticketId={ticket.id} />
      </section>
    </div>
  );
}
