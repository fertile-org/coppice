import React from "react";
import { COLUMNS, type ColumnName } from "./copy";
import { columnClasses } from "./columnClasses";

// Default: web/src/components/ui/badge.tsx (default variant).
// With `status`: web/src/features/tickets/TicketStatusBadge.tsx, the column
// coloured pill used in the ticket drawer header and Status field.
export const Badge: React.FC<{ status?: ColumnName; children?: React.ReactNode }> = ({
  status,
  children,
}) => {
  if (status) {
    const column = COLUMNS.find((c) => c.name === status)!;
    const c = columnClasses(column.colorKey);
    return (
      <span
        className={`inline-flex items-center rounded-full border px-2.5 py-0.5 font-body text-xs font-semibold ${c.bg} ${c.border} ${c.accent}`}
      >
        {status}
      </span>
    );
  }
  return (
    <div className="border-border bg-surface text-text-secondary inline-flex items-center rounded-full border px-2.5 py-0.5 font-body text-xs font-medium">
      {children}
    </div>
  );
};
