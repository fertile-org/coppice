import type { Ticket } from './useTickets';

export interface TicketParentSummary {
  id: string;
  title: string;
}

export interface TicketHierarchy {
  parent: TicketParentSummary | null;
  parentUnavailable: boolean;
  directChildCount: number;
  doneChildCount: number;
}

export type TicketHierarchyIndex = ReadonlyMap<string, TicketHierarchy>;

function emptyHierarchy(): TicketHierarchy {
  return {
    parent: null,
    parentUnavailable: false,
    directChildCount: 0,
    doneChildCount: 0,
  };
}

export function buildTicketHierarchyIndex(
  tickets: readonly Ticket[],
): TicketHierarchyIndex {
  const ticketsById = new Map(tickets.map((ticket) => [ticket.id, ticket]));
  const hierarchyByTicketId = new Map(
    tickets.map((ticket) => [ticket.id, emptyHierarchy()]),
  );

  for (const ticket of tickets) {
    if (!ticket.parentTicketId) continue;

    const ticketHierarchy = hierarchyByTicketId.get(ticket.id);
    if (!ticketHierarchy) continue;

    const parent = ticketsById.get(ticket.parentTicketId);
    if (!parent) {
      hierarchyByTicketId.set(ticket.id, {
        ...ticketHierarchy,
        parentUnavailable: true,
      });
      continue;
    }

    hierarchyByTicketId.set(ticket.id, {
      ...ticketHierarchy,
      parent: { id: parent.id, title: parent.title },
    });

    const parentHierarchy = hierarchyByTicketId.get(parent.id);
    if (parentHierarchy) {
      hierarchyByTicketId.set(parent.id, {
        ...parentHierarchy,
        directChildCount: parentHierarchy.directChildCount + 1,
        doneChildCount:
          parentHierarchy.doneChildCount + (ticket.status === 'done' ? 1 : 0),
      });
    }
  }

  return hierarchyByTicketId;
}
