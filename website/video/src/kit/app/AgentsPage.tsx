import React from "react";
import { ComboboxTrigger } from "./Combobox";
import { AGENTS, KNOWLEDGE_COMPACTION } from "./copy";

// Mirrors web/src/features/agents/AgentsPage.tsx for an admin with agents loaded:
// header (Agents, subtitle, New agent), KnowledgeCompactionCard (always rendered above the table once
// the agents load; here simplified to its admin view with compaction off: title, description,
// Compaction agent combobox showing "None — compaction off", Save disabled while nothing changed,
// the status line), then the agents table (thead on bg-paper-100; AgentRow: name with the preset key
// in mono under it, role, connector id with the mono model line `{provider}/{model}` under it, Enabled
// and Healthy pills, Updated date, Disable / Edit). `scrollTop` is the <main> scroll.
//
// Render as the child of <AppShell active="Agents">. Dialogs go in `children`, which are
// positioned against the window, not the scrolled page.

export type AgentRowData = {
  name: string;
  presetSource: string;
  role: string;
  /** The connector id, as the table prints it (e.g. "claude-code"). */
  connector: string;
  model?: { provider: string; id: string };
  /** formatDate(updatedAt), e.g. "Nov 21, 2026". */
  updated: string;
};

const TH = "px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted";
const PILL = "inline-flex rounded-full px-2 py-0.5 font-body text-xs font-medium bg-moss-100 text-moss-800";
const ROW_BUTTON = "rounded-md border border-border px-2.5 py-1 font-body text-xs";

export const AgentsPage: React.FC<{
  agents: AgentRowData[];
  scrollTop?: number;
  /** The row whose Edit button is under the pointer: the row's hover:bg-paper-100 and the button's
   * hover:text-text-primary. */
  editHover?: string;
  children?: React.ReactNode;
}> = ({ agents, scrollTop = 0, editHover, children }) => (
  <>
    <div style={{ translate: `0px ${-scrollTop}px` }}>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-display text-2xl font-semibold text-bark-900">{AGENTS.title}</h1>
          <p className="mt-2 max-w-xl font-body text-text-secondary">{AGENTS.subtitle}</p>
        </div>
        <div className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm">
          {AGENTS.newAgent}
        </div>
      </div>

      <section className="mt-8 rounded-xl border border-border bg-surface-raised p-5 shadow-card">
        <h2 className="font-display text-lg font-semibold text-bark-900">{KNOWLEDGE_COMPACTION.title}</h2>
        <p className="mt-1 max-w-2xl font-body text-sm text-text-secondary">{KNOWLEDGE_COMPACTION.description}</p>
        <div className="mt-4 flex flex-wrap items-end gap-3">
          <div className="min-w-[16rem] max-w-md flex-1">
            <p className="mb-1 block font-body text-sm font-medium text-bark-800">{KNOWLEDGE_COMPACTION.agentLabel}</p>
            <ComboboxTrigger label={KNOWLEDGE_COMPACTION.none} muted />
          </div>
          <div
            className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm"
            style={{ opacity: 0.6 }}
          >
            {KNOWLEDGE_COMPACTION.save}
          </div>
        </div>
        <p className="mt-4 font-body text-xs text-text-muted">{KNOWLEDGE_COMPACTION.status}</p>
      </section>

      <div className="mt-8 overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-border bg-paper-100">
              {AGENTS.columns.map((column) => (
                <th key={column} className={TH}>
                  {column}
                </th>
              ))}
              <th className="px-4 py-3" />
            </tr>
          </thead>
          <tbody>
            {agents.map((agent, i) => (
              <tr
                key={agent.name}
                className={[
                  i < agents.length - 1 ? "border-b border-border" : "",
                  editHover === agent.name ? "bg-paper-100" : "",
                ].join(" ")}
              >
                <td className="px-4 py-3">
                  <div className="flex flex-wrap items-center gap-2 font-body text-sm font-medium text-text-primary">
                    {agent.name}
                  </div>
                  <div className="mt-0.5 font-mono text-xs text-text-muted">{agent.presetSource}</div>
                </td>
                <td className="px-4 py-3 font-body text-sm text-text-secondary">{agent.role}</td>
                <td className="px-4 py-3 font-body text-sm text-text-secondary">
                  <div>{agent.connector}</div>
                  {agent.model && (
                    <div className="mt-0.5 font-mono text-xs text-text-muted">
                      {AGENTS.modelLine(agent.model.provider, agent.model.id)}
                    </div>
                  )}
                </td>
                <td className="px-4 py-3">
                  <span className={PILL}>{AGENTS.enabled}</span>
                </td>
                <td className="px-4 py-3">
                  <span className={PILL}>{AGENTS.healthy}</span>
                </td>
                <td className="px-4 py-3 font-body text-xs text-text-muted">{agent.updated}</td>
                <td className="px-4 py-3">
                  <div className="flex items-center justify-end gap-2">
                    <div className={`${ROW_BUTTON} text-text-secondary`}>{AGENTS.disable}</div>
                    <div
                      className={`${ROW_BUTTON} ${editHover === agent.name ? "text-text-primary" : "text-text-secondary"}`}
                    >
                      {AGENTS.edit}
                    </div>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
    {children}
  </>
);
