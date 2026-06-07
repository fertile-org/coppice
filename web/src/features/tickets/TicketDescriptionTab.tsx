import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Ticket } from '../board/useTickets';
import { useAgents, useAssignAgent, useUpdateTicket } from './useTicket';

interface TicketDescriptionTabProps {
  ticket: Ticket;
}

export function TicketDescriptionTab({ ticket }: TicketDescriptionTabProps) {
  const updateTicket = useUpdateTicket(ticket.id);
  const assignAgent = useAssignAgent(ticket.id);
  const { data: agents } = useAgents();
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(ticket.title);
  const [description, setDescription] = useState(ticket.description);
  const [assigneeId, setAssigneeId] = useState(ticket.assigneeAgentId ?? '');
  const [assignError, setAssignError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTitle(ticket.title);
    setDescription(ticket.description);
    setAssigneeId(ticket.assigneeAgentId ?? '');
  }, [ticket.title, ticket.description, ticket.assigneeAgentId]);

  async function handleSave() {
    setError(null);
    try {
      await updateTicket.mutateAsync({
        title: title.trim(),
        description,
      });
      setEditing(false);
    } catch {
      setError('Unable to save changes.');
    }
  }

  function handleCancel() {
    setTitle(ticket.title);
    setDescription(ticket.description);
    setEditing(false);
    setError(null);
  }

  async function handleAssignChange(nextAgentId: string) {
    setAssignError(null);
    setAssigneeId(nextAgentId);
    try {
      await assignAgent.mutateAsync(nextAgentId || null);
    } catch {
      setAssigneeId(ticket.assigneeAgentId ?? '');
      setAssignError('Unable to update assignee.');
    }
  }

  const assignedAgent = agents?.find((agent) => agent.id === assigneeId);

  return (
    <div className="flex h-full flex-col gap-4">
      <label className="block space-y-1.5">
        <span className="font-body text-sm font-medium text-text-secondary">
          Assignee
        </span>
        <select
          value={assigneeId}
          onChange={(e) => void handleAssignChange(e.target.value)}
          disabled={assignAgent.isPending}
          className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted disabled:opacity-50"
        >
          <option value="">Unassigned</option>
          {agents
            ?.filter((agent) => agent.enabled)
            .map((agent) => (
              <option key={agent.id} value={agent.id}>
                {agent.name} ({agent.role})
              </option>
            ))}
        </select>
        {assignedAgent && (
          <p className="font-body text-xs text-text-muted">
            Assigned to {assignedAgent.name}
          </p>
        )}
      </label>

      {assignError && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {assignError}
        </p>
      )}

      <div className="flex items-start justify-between gap-4">
        {editing ? (
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-display text-lg font-semibold text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          />
        ) : (
          <h3 className="font-display text-lg font-semibold text-text-primary">
            {ticket.title}
          </h3>
        )}

        <div className="flex shrink-0 gap-2">
          {editing ? (
            <>
              <button
                type="button"
                onClick={handleCancel}
                disabled={updateTicket.isPending}
                className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={updateTicket.isPending || title.trim().length === 0}
                className="rounded-md bg-accent px-3 py-1.5 font-body text-sm font-medium text-white transition-colors duration-fast hover:bg-accent-hover disabled:opacity-50"
              >
                {updateTicket.isPending ? 'Saving…' : 'Save'}
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Edit
            </button>
          )}
        </div>
      </div>

      {error && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {error}
        </p>
      )}

      {editing ? (
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={16}
          placeholder="Describe the ticket in markdown…"
          className="min-h-0 flex-1 resize-y rounded-md border border-border bg-surface px-3 py-2 font-mono text-sm leading-relaxed text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto rounded-md border border-border bg-surface px-4 py-3 [&_a]:text-accent [&_code]:rounded [&_code]:bg-paper-200 [&_code]:px-1 [&_p+p]:mt-3">
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
  );
}
