import { useEffect, useState } from 'react';
import { BOARD_COLUMNS } from '../board/columns';
import type { Ticket } from '../board/useTickets';
import {
  SUBSTATUSES,
  SUBSTATUS_LABELS,
  substatusMetadataSchema,
  substatusOptionalReason,
  substatusRequiresMetadata,
  type Substatus,
} from '../../lib/schemas/substatus';
import { ticketPrioritySchema } from '../../lib/schemas/ticket';
import { useAgents, useUpdateTicket, useUpdateTicketStatus } from './useTicket';

interface TicketMetadataTabProps {
  ticket: Ticket;
}

function metadataFromTicket(ticket: Ticket): Record<string, unknown> {
  return ticket.substatusMetadata ?? {};
}

export function TicketMetadataTab({ ticket }: TicketMetadataTabProps) {
  const updateTicket = useUpdateTicket(ticket.id);
  const updateStatus = useUpdateTicketStatus(ticket.id);
  const { data: agents } = useAgents();

  const [status, setStatus] = useState(ticket.status);
  const [substatus, setSubstatus] = useState<Substatus | ''>(
    (ticket.substatus as Substatus | undefined) ?? '',
  );
  const [metadata, setMetadata] = useState<Record<string, unknown>>(
    metadataFromTicket(ticket),
  );
  const [priority, setPriority] = useState(ticket.priority ?? '');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setStatus(ticket.status);
    setSubstatus((ticket.substatus as Substatus | undefined) ?? '');
    setMetadata(metadataFromTicket(ticket));
    setPriority(ticket.priority ?? '');
  }, [ticket]);

  function updateMetadataField(key: string, value: string) {
    setMetadata((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSave() {
    setError(null);

    const parsedPriority = priority
      ? ticketPrioritySchema.safeParse(priority)
      : { success: true as const, data: null };

    if (!parsedPriority.success) {
      setError('Invalid priority.');
      return;
    }

    if (substatus) {
      const validation = substatusMetadataSchema.safeParse({ substatus, metadata });
      if (!validation.success) {
        setError(validation.error.issues[0]?.message ?? 'Invalid substatus metadata.');
        return;
      }
    }

    try {
      await updateStatus.mutateAsync({
        status,
        substatus: substatus || null,
        substatusMetadata: substatus ? metadata : undefined,
      });

      if (parsedPriority.data !== ticket.priority) {
        await updateTicket.mutateAsync({
          priority: parsedPriority.data,
        });
      }
    } catch {
      setError('Unable to save metadata.');
    }
  }

  const isBusy = updateStatus.isPending || updateTicket.isPending;
  const activeSubstatus = substatus || null;

  return (
    <div className="space-y-6">
      {error && (
        <p className="rounded-md border border-danger-muted bg-danger-muted/40 px-3 py-2 font-body text-sm text-danger">
          {error}
        </p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Status
          </span>
          <select
            value={status}
            onChange={(e) => setStatus(e.target.value as typeof status)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          >
            {BOARD_COLUMNS.map((column) => (
              <option key={column.status} value={column.status}>
                {column.label}
              </option>
            ))}
          </select>
        </label>

        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Priority
          </span>
          <select
            value={priority}
            onChange={(e) => setPriority(e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm capitalize text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          >
            <option value="">None</option>
            {ticketPrioritySchema.options.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </label>
      </div>

      <label className="block space-y-1.5">
        <span className="font-body text-sm font-medium text-text-secondary">
          Substatus
        </span>
        <select
          value={substatus}
          onChange={(e) => {
            const next = e.target.value as Substatus | '';
            setSubstatus(next);
            if (!next) setMetadata({});
          }}
          className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
        >
          <option value="">None</option>
          {SUBSTATUSES.map((value) => (
            <option key={value} value={value}>
              {SUBSTATUS_LABELS[value]}
            </option>
          ))}
        </select>
      </label>

      {activeSubstatus === 'waiting_for_agent' && (
        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Agent
          </span>
          <select
            value={String(metadata.agentId ?? '')}
            onChange={(e) => updateMetadataField('agentId', e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          >
            <option value="">Select agent…</option>
            {agents
              ?.filter((agent) => agent.enabled)
              .map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.name} ({agent.role})
                </option>
              ))}
          </select>
        </label>
      )}

      {activeSubstatus === 'blocked_by_missing_capability' && (
        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Capability
          </span>
          <input
            type="text"
            value={String(metadata.capability ?? '')}
            onChange={(e) => updateMetadataField('capability', e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          />
        </label>
      )}

      {activeSubstatus === 'blocked_by_missing_secret' && (
        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Secret key
          </span>
          <input
            type="text"
            value={String(metadata.secretKey ?? '')}
            onChange={(e) => updateMetadataField('secretKey', e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          />
        </label>
      )}

      {activeSubstatus && substatusOptionalReason(activeSubstatus) && (
        <label className="block space-y-1.5">
          <span className="font-body text-sm font-medium text-text-secondary">
            Reason (optional)
          </span>
          <input
            type="text"
            value={String(metadata.reason ?? '')}
            onChange={(e) => updateMetadataField('reason', e.target.value)}
            className="w-full rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted"
          />
        </label>
      )}

      {activeSubstatus && substatusRequiresMetadata(activeSubstatus) && (
        <p className="font-body text-xs text-text-muted">
          {SUBSTATUS_LABELS[activeSubstatus]} requires additional metadata before
          saving.
        </p>
      )}

      <button
        type="button"
        onClick={() => void handleSave()}
        disabled={isBusy}
        className="rounded-md bg-accent px-4 py-2 font-body text-sm font-medium text-white transition-colors duration-fast hover:bg-accent-hover disabled:opacity-50"
      >
        {isBusy ? 'Saving…' : 'Save metadata'}
      </button>
    </div>
  );
}
