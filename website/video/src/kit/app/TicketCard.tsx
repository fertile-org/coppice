import { Bot } from "lucide-react";
import React from "react";
import { PLAN } from "./copy";

export type Ticket = {
  id: string;
  title: string;
  /** Assignee display name, shown with the Bot icon (e.g. "PM"). */
  agent?: string;
  /** The card's plan-state line. Use a PLAN card string from ./copy (e.g. PLAN.writingPlan). */
  status?: string;
  priority?: "low" | "medium" | "high" | "critical";
  /** Agent running: the dot before the title. */
  live?: boolean;
};

// Mirrors web/src/features/board/TicketCard.tsx: title, assignee, priority pill,
// plan-state line. Hover, drag and transition classes dropped.
// `highlighted` is not app UI: it is the amber outline the app's own hero
// screenshots draw around the card they talk about
// (e2e/screenshot/capture-hero.mjs), here in the warning token.
export const TicketCard: React.FC<{ ticket: Ticket; highlighted?: boolean }> = ({
  ticket,
  highlighted = false,
}) => (
  <div
    className="rounded-md border border-border bg-surface-raised p-3 shadow-card"
    style={
      highlighted
        ? { outline: "3px solid var(--color-warning)", outlineOffset: 3 }
        : undefined
    }
  >
    <p className="flex items-start gap-1.5 font-body text-sm font-medium leading-snug text-text-primary">
      {ticket.live && (
        <span className="mt-1.5 inline-block h-2 w-2 shrink-0 rounded-full bg-accent" />
      )}
      <span className="min-w-0 flex-1">{ticket.title}</span>
    </p>

    {ticket.agent && (
      <div className="mt-1.5 flex min-w-0 items-center gap-1.5 font-body text-xs leading-tight text-text-secondary">
        <Bot className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        <span className="min-w-0 truncate">{ticket.agent}</span>
      </div>
    )}

    {ticket.priority && (
      <div className="mt-2 flex flex-wrap gap-1.5">
        <span
          className="rounded-full border px-2 py-0.5 font-body text-xs capitalize"
          style={{
            backgroundColor: `var(--badge-priority-${ticket.priority}-bg)`,
            color: `var(--badge-priority-${ticket.priority}-text)`,
            borderColor: `var(--badge-priority-${ticket.priority}-border)`,
          }}
        >
          {ticket.priority}
        </span>
      </div>
    )}

    {ticket.status && (
      <div className="mt-2">
        <p className="font-body text-xs leading-tight text-text-secondary">{ticket.status}</p>
        {/* On the board "Plan needs a run" always comes with a button; Run agent is
            the one shown when no PM is suggested. */}
        {ticket.status === PLAN.planNeedsRun && (
          <div className="mt-1.5 inline-block rounded-md border border-border bg-surface px-2 py-1 font-body text-xs font-medium text-text-primary">
            {PLAN.runAgent}
          </div>
        )}
      </div>
    )}
  </div>
);
