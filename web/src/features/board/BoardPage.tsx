import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useSession } from '../auth/useSession';
import { TicketDrawer } from '../tickets/TicketDrawer';
import { useEventSocket } from '../ws/useEventSocket';
import { setLastProjectId } from '../projects/useProjects';
import { BoardColumn } from './BoardColumn';
import { BOARD_COLUMNS, isTicketStatus, type TicketStatus } from './columns';
import { TicketCard } from './TicketCard';
import {
  ticketsQueryKey,
  useCreateTicket,
  useTickets,
  useUpdateTicketStatus,
  type Ticket,
} from './useTickets';

function resolveDropStatus(
  overId: string | number,
  ticketsById: Map<string, Ticket>,
): TicketStatus | null {
  const id = String(overId);
  if (isTicketStatus(id)) return id;
  const ticket = ticketsById.get(id);
  return ticket?.status ?? null;
}

export function BoardPage() {
  const { projectId } = useParams<{ projectId: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const { user } = useSession();
  const selectedTicketId = searchParams.get('ticket');

  const { data: tickets, isLoading, isError, refetch } = useTickets(projectId);
  const createTicket = useCreateTicket(projectId ?? '');
  const updateStatus = useUpdateTicketStatus(projectId ?? '');

  const [activeTicket, setActiveTicket] = useState<Ticket | null>(null);
  const [runningTicketIds, setRunningTicketIds] = useState<Set<string>>(
    () => new Set(),
  );

  const handleRunStarted = useCallback(
    (payload: { ticket_id: string }) => {
      setRunningTicketIds((current) => new Set(current).add(payload.ticket_id));
    },
    [],
  );

  const handleRunFinished = useCallback(
    (payload: { ticket_id: string }) => {
      setRunningTicketIds((current) => {
        const next = new Set(current);
        next.delete(payload.ticket_id);
        return next;
      });
    },
    [],
  );

  useEventSocket({
    enabled: Boolean(user),
    onRunStarted: handleRunStarted,
    onRunFinished: handleRunFinished,
  });

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    }),
  );

  useEffect(() => {
    if (projectId) setLastProjectId(projectId);
  }, [projectId]);

  const ticketsById = useMemo(
    () => new Map((tickets ?? []).map((t) => [t.id, t])),
    [tickets],
  );

  const ticketsByStatus = useMemo(() => {
    const grouped = new Map<TicketStatus, Ticket[]>(
      BOARD_COLUMNS.map((c) => [c.status, []]),
    );
    for (const ticket of tickets ?? []) {
      const list = grouped.get(ticket.status);
      if (list) list.push(ticket);
    }
    return grouped;
  }, [tickets]);

  function openTicket(ticketId: string) {
    setSearchParams({ ticket: ticketId });
  }

  function closeDrawer() {
    setSearchParams({});
    if (projectId) {
      void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(projectId) });
    }
  }

  function handleDragStart(event: DragStartEvent) {
    const ticket = event.active.data.current?.ticket as Ticket | undefined;
    setActiveTicket(ticket ?? ticketsById.get(String(event.active.id)) ?? null);
  }

  function handleDragEnd(event: DragEndEvent) {
    setActiveTicket(null);
    const { active, over } = event;
    if (!over || !projectId) return;

    const ticketId = String(active.id);
    const ticket = ticketsById.get(ticketId);
    if (!ticket) return;

    const targetStatus = resolveDropStatus(over.id, ticketsById);
    if (!targetStatus || targetStatus === ticket.status) return;

    void updateStatus.mutateAsync({ ticketId, status: targetStatus });
  }

  async function handleQuickAdd(title: string) {
    await createTicket.mutateAsync(title);
  }

  if (!projectId) {
    return (
      <p className="font-body text-sm text-danger">Missing project id.</p>
    );
  }

  return (
    <div>
      <div className="mb-6">
        <h1 className="font-display text-2xl font-semibold text-text-primary">
          Board
        </h1>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Drag tickets between columns to update status.
        </p>
      </div>

      {isLoading && (
        <p className="font-body text-sm text-text-muted">Loading tickets…</p>
      )}

      {isError && (
        <div className="rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
          <p className="font-body text-sm text-danger">
            Unable to load tickets.
          </p>
          <button
            type="button"
            onClick={() => void refetch()}
            className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Try again
          </button>
        </div>
      )}

      {!isLoading && !isError && (
        <DndContext
          sensors={sensors}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
        >
          <div className="-mx-8 overflow-x-auto px-8 pb-4">
            <div className="flex min-w-max gap-4">
              {BOARD_COLUMNS.map((column) => (
                <BoardColumn
                  key={column.status}
                  column={column}
                  tickets={ticketsByStatus.get(column.status) ?? []}
                  showQuickAdd={column.status === 'backlog'}
                  onQuickAdd={handleQuickAdd}
                  isAdding={createTicket.isPending}
                  onOpenTicket={openTicket}
                  liveTicketIds={runningTicketIds}
                />
              ))}
            </div>
          </div>

          <DragOverlay>
            {activeTicket ? (
              <div className="w-72 rotate-2 opacity-95">
                <TicketCard
                  ticket={activeTicket}
                  onOpen={() => {}}
                  isLive={runningTicketIds.has(activeTicket.id)}
                />
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      )}

      {selectedTicketId && (
        <TicketDrawer ticketId={selectedTicketId} onClose={closeDrawer} />
      )}
    </div>
  );
}
