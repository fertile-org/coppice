import { ListFilter } from "lucide-react";
import React from "react";
import { Button } from "./Button";
import { columnClasses } from "./columnClasses";
import { COLUMNS, type ColumnName } from "./copy";
import { TicketCard, type Ticket } from "./TicketCard";

export type BoardColumn = { name: ColumnName; tickets: Ticket[] };

// Mirrors web/src/features/board/BoardPage.tsx (page header, Filter button) and
// BoardColumn.tsx (column frame, header with count, Backlog quick add).
// Column height: the app's min-h-[calc(100svh-14rem)] at the 800px logical
// window height of AppShell (marketing kit).
export const Board: React.FC<{
  columns: BoardColumn[];
  /** Ticket ids drawn with the highlight outline. */
  highlightedIds?: string[];
}> = ({ columns, highlightedIds = [] }) => (
  <div>
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="font-display text-2xl font-semibold text-text-primary">Board</h1>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Drag tickets between columns to update status.
        </p>
      </div>
      <div className="flex gap-2">
        <Button variant="secondary">
          <ListFilter className="size-4" aria-hidden="true" />
          Filter
        </Button>
      </div>
    </div>

    <div className="-mx-8 px-8 pb-4">
      <div className="flex gap-4">
        {columns.map((column) => {
          const def = COLUMNS.find((c) => c.name === column.name);
          if (!def) throw new Error(`Not a Coppice column: ${column.name}`);
          const c = columnClasses(def.colorKey);
          return (
            <section
              key={column.name}
              className={`flex min-h-[576px] w-72 shrink-0 flex-col rounded-lg border ${c.bg} ${c.border}`}
            >
              <header
                className={`flex items-center justify-between gap-2 border-b px-3 py-2.5 ${c.border}`}
              >
                <h2 className={`rounded-sm font-display text-sm font-semibold ${c.accent}`}>
                  {column.name}
                </h2>
                <span className={`rounded-full px-2 py-0.5 font-mono text-xs ${c.bg} ${c.accent}`}>
                  {column.tickets.length}
                </span>
              </header>

              <div className="flex flex-1 flex-col gap-2 p-2">
                {def.colorKey === "backlog" && (
                  <div className="mb-1 border-b border-border/60 pb-2">
                    <div
                      className="w-full rounded-md border px-2 py-1.5 font-body text-sm text-text-muted"
                      style={{
                        borderColor: "var(--color-field-border)",
                        backgroundColor: "var(--color-field-bg)",
                        boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.06)",
                      }}
                    >
                      Add ticket…
                    </div>
                  </div>
                )}
                {column.tickets.map((ticket) => (
                  <TicketCard
                    key={ticket.id}
                    ticket={ticket}
                    highlighted={highlightedIds.includes(ticket.id)}
                  />
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  </div>
);
