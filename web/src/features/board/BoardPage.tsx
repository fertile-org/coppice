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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useToast } from '../../components/ToastProvider';
import { apiErrorToastMessage, parseApiErrorMessage } from '../../lib/api';
import { useAgents } from '../agents/useAgents';
import { useKeyboardEnvironment, useKeyboardLayer } from '../keyboard/useKeyboard';
import { TicketDrawer } from '../tickets/TicketDrawer';
import { setLastBoardId } from '../boards/useBoards';
import {
  BoardFilterButton,
  BoardFilterDrawer,
} from './BoardFilterDrawer';
import {
  countActiveBoardFilters,
  DEFAULT_BOARD_FILTERS,
  filterTickets,
  includeArchivedForVisibility,
  parseBoardFilters,
  writeBoardFilters,
  type BoardFilters,
} from './boardFilters';
import {
  boardFocusDomId,
  movedToColumnAnnouncement,
  reconcileBoardFocus,
  type BoardColumnSnapshot,
  type BoardFocus,
} from './boardFocus';
import { BoardColumn } from './BoardColumn';
import { useBoardKeyboard } from './useBoardKeyboard';
import { ReviewCapacityBar } from './ReviewCapacityBar';
import { BOARD_COLUMNS, isTicketStatus, type TicketStatus } from './columns';
import { resolveAssigneeName, TicketCard } from './TicketCard';
import { buildTicketHierarchyIndex } from './ticketHierarchy';
import {
  ticketsQueryKey,
  useCreateTicket,
  useStartPlanRun,
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
  const { boardId } = useParams<{ boardId: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const selectedTicketId = searchParams.get('ticket');
  const [filterOpen, setFilterOpen] = useState(false);

  const filters = useMemo(
    () => parseBoardFilters(searchParams),
    [searchParams],
  );
  const includeArchived = includeArchivedForVisibility(filters.visibility);
  const activeFilterCount = countActiveBoardFilters(filters);

  const { data: tickets, isLoading, isError, refetch } = useTickets(
    boardId,
    includeArchived,
  );
  const { data: agents } = useAgents();
  const toast = useToast();
  const createTicket = useCreateTicket(boardId ?? '');
  const updateStatus = useUpdateTicketStatus(boardId ?? '', includeArchived);
  const startPlan = useStartPlanRun(boardId ?? '');

  const [activeTicket, setActiveTicket] = useState<Ticket | null>(null);
  const [focusState, setFocusState] = useState<BoardFocus | null>(null);
  const [focusGeneration, setFocusGeneration] = useState(0);
  const [moveAnnouncement, setMoveAnnouncement] = useState('');
  const returnTicketId = useRef<string | null>(null);

  const agentsById = useMemo(() => {
    const map = new Map<string, string>();
    for (const agent of agents ?? []) {
      map.set(agent.id, agent.name);
    }
    return map;
  }, [agents]);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    }),
  );

  useEffect(() => {
    if (boardId) setLastBoardId(boardId);
  }, [boardId]);

  const filteredTickets = useMemo(
    () => filterTickets(tickets ?? [], filters),
    [tickets, filters],
  );

  const ticketsById = useMemo(
    () => new Map((tickets ?? []).map((t) => [t.id, t])),
    [tickets],
  );

  const hierarchyIndex = useMemo(
    () => buildTicketHierarchyIndex(tickets ?? []),
    [tickets],
  );

  const ticketsByStatus = useMemo(() => {
    const grouped = new Map<TicketStatus, Ticket[]>(
      BOARD_COLUMNS.map((c) => [c.status, []]),
    );
    for (const ticket of filteredTickets) {
      const list = grouped.get(ticket.status);
      if (list) list.push(ticket);
    }
    return grouped;
  }, [filteredTickets]);

  const columnSnapshots = useMemo<BoardColumnSnapshot[]>(
    () =>
      BOARD_COLUMNS.map((column) => ({
        ticketIds: (ticketsByStatus.get(column.status) ?? []).map((ticket) => ticket.id),
      })),
    [ticketsByStatus],
  );
  const boardFocus = reconcileBoardFocus(columnSnapshots, focusState);
  const closeDrawer = useCallback(() => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete('ticket');
      return next;
    });
    if (boardId) {
      void queryClient.invalidateQueries({ queryKey: ticketsQueryKey(boardId) });
    }
    const ticketId = returnTicketId.current;
    if (!ticketId) return;
    setFocusState({ type: 'card', ticketId });
    setFocusGeneration((generation) => generation + 1);
  }, [boardId, queryClient, setSearchParams]);

  const columnsRef = useRef(columnSnapshots);
  const focusRef = useRef(boardFocus);
  const focusDomIdRef = useRef(boardFocusDomId(boardFocus));
  const closeDrawerRef = useRef(closeDrawer);

  useEffect(() => {
    columnsRef.current = columnSnapshots;
    focusRef.current = boardFocus;
    focusDomIdRef.current = boardFocusDomId(boardFocus);
    closeDrawerRef.current = closeDrawer;
  }, [columnSnapshots, boardFocus, closeDrawer]);

  const drawerOpen = Boolean(selectedTicketId) && !filterOpen;
  useKeyboardEnvironment({ drawerOpen, modalOpen: filterOpen });

  function moveBoardFocus(next: BoardFocus) {
    setFocusState(next);
    setFocusGeneration((generation) => generation + 1);
  }

  useBoardKeyboard({
    enabled: !drawerOpen && !filterOpen,
    columnsRef,
    focusRef,
    onMove: moveBoardFocus,
    onOpenTicket: openTicket,
  });

  useEffect(() => {
    if (focusGeneration === 0) return;
    const node = document.querySelector(
      `[data-board-focus-id="${CSS.escape(focusDomIdRef.current)}"]`,
    );
    if (!(node instanceof HTMLElement)) return;
    node.focus();
    node.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [focusGeneration]);

  const [shownTicketId, setShownTicketId] = useState(selectedTicketId);
  if (selectedTicketId && selectedTicketId !== shownTicketId) {
    setShownTicketId(selectedTicketId);
  }
  const shownParentTicket = shownTicketId
    ? (hierarchyIndex.get(shownTicketId)?.parent ?? null)
    : null;

  function updateFilters(next: BoardFilters) {
    setSearchParams(writeBoardFilters(searchParams, next), { replace: true });
  }

  function clearFilters() {
    setSearchParams(writeBoardFilters(searchParams, DEFAULT_BOARD_FILTERS), {
      replace: true,
    });
  }

  function openTicket(ticketId: string) {
    returnTicketId.current = ticketId;
    setFocusState({ type: 'card', ticketId });
    setFilterOpen(false);
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.set('ticket', ticketId);
      return next;
    });
  }

  function restoreBoardFocus() {
    const ticketId = returnTicketId.current;
    if (!ticketId) return;
    moveBoardFocus({ type: 'card', ticketId });
  }

  useKeyboardLayer({
    closeDrawer: () => closeDrawerRef.current(),
  });

  function openFilters() {
    if (selectedTicketId) {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.delete('ticket');
        return next;
      });
    }
    setFilterOpen(true);
  }

  function handleDragStart(event: DragStartEvent) {
    const ticket = event.active.data.current?.ticket as Ticket | undefined;
    setActiveTicket(ticket ?? ticketsById.get(String(event.active.id)) ?? null);
  }

  function handleDragEnd(event: DragEndEvent) {
    setActiveTicket(null);
    const { active, over } = event;
    if (!over || !boardId) return;

    const ticketId = String(active.id);
    const ticket = ticketsById.get(ticketId);
    if (!ticket || ticket.archivedAt) return;

    const targetStatus = resolveDropStatus(over.id, ticketsById);
    if (!targetStatus || targetStatus === ticket.status) return;

    const columnLabel = BOARD_COLUMNS.find((column) => column.status === targetStatus)
      ?.label;
    void updateStatus
      .mutateAsync({ ticketId, status: targetStatus })
      .then(() => {
        if (columnLabel) setMoveAnnouncement(movedToColumnAnnouncement(columnLabel));
      })
      .catch((err: unknown) => {
        toast.error(apiErrorToastMessage(parseApiErrorMessage(err)));
      });
  }

  async function handleQuickAdd(title: string) {
    await createTicket.mutateAsync(title);
  }

  function handleRunPlan(ticketId: string) {
    void startPlan.mutateAsync(ticketId).catch((err: unknown) => {
      toast.error(apiErrorToastMessage(parseApiErrorMessage(err)));
    });
  }

  if (!boardId) {
    return (
      <p className="font-body text-sm text-danger">Missing board id.</p>
    );
  }

  return (
    <div>
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="font-display text-2xl font-semibold text-text-primary">
            Board
          </h1>
          <p className="mt-1 font-body text-sm text-text-secondary">
            Drag tickets between columns to update status.
          </p>
          <ReviewCapacityBar boardId={boardId} />
        </div>
        <BoardFilterButton
          activeCount={activeFilterCount}
          onClick={openFilters}
        />
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
          <div className="sr-only" aria-live="polite" aria-atomic="true">
            {moveAnnouncement}
          </div>
          <div id="board" className="-mx-8 overflow-x-auto px-8 pb-4">
            <div className="flex min-w-max gap-4">
              {BOARD_COLUMNS.map((column, columnIndex) => (
                <BoardColumn
                  key={column.status}
                  column={column}
                  columnIndex={columnIndex}
                  tickets={ticketsByStatus.get(column.status) ?? []}
                  hierarchyIndex={hierarchyIndex}
                  agentsById={agentsById}
                  showQuickAdd={column.status === 'backlog'}
                  onQuickAdd={handleQuickAdd}
                  isAdding={createTicket.isPending}
                  onOpenTicket={openTicket}
                  onRunPlan={handleRunPlan}
                  planRunTicketId={
                    startPlan.isPending ? (startPlan.variables ?? null) : null
                  }
                  boardFocus={boardFocus}
                  onCardFocus={(ticketId) =>
                    setFocusState((current) =>
                      current?.type === 'card' && current.ticketId === ticketId
                        ? current
                        : { type: 'card', ticketId },
                    )
                  }
                  onHeadingFocus={(index) =>
                    setFocusState((current) =>
                      current?.type === 'heading' && current.column === index
                        ? current
                        : { type: 'heading', column: index },
                    )
                  }
                />
              ))}
            </div>
          </div>

          <DragOverlay>
            {activeTicket ? (
              <div className="w-72 rotate-2 opacity-95">
                <TicketCard
                  ticket={activeTicket}
                  hierarchy={hierarchyIndex.get(activeTicket.id)}
                  onOpen={() => {}}
                  isLive={activeTicket.hasActiveRun ?? false}
                  assigneeName={resolveAssigneeName(
                    activeTicket.assigneeAgentId,
                    agentsById,
                  )}
                  tabIndex={-1}
                />
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      )}

      {shownTicketId && (
        <TicketDrawer
          open={Boolean(selectedTicketId) && !filterOpen}
          ticketId={shownTicketId}
          parentTicket={shownParentTicket}
          onClose={closeDrawer}
          onRestoreFocus={restoreBoardFocus}
        />
      )}

      <BoardFilterDrawer
        open={filterOpen}
        filters={filters}
        onChange={updateFilters}
        onClose={() => setFilterOpen(false)}
        onClear={clearFilters}
      />
    </div>
  );
}
