import { Check } from "lucide-react";
import React from "react";
import { ComboboxList, ComboboxTrigger, FIELD } from "./Combobox";
import { AGENTS } from "./copy";

// Mirrors EditAgentDialog in web/src/features/agents/AgentsPage.tsx: DialogContent
// (web/src/components/ui/dialog.tsx; `flex max-h-[90vh] max-w-3xl flex-col overflow-hidden`, so 648 × 768 px
// in the 720 px window, p-6, no close button) holding AgentForm.tsx in edit mode. The form body scrolls
// (`scrollTop`); the footer (DialogFooter: Cancel, Save changes) stays. Fields in app order: header
// (Edit agent, "Update {name}'s configuration."), Name, Role, Skills (no enabled plugins), Responsibilities
// (rows 3), System prompt (rows 20, mono), Connector, Model with Refresh models, Thinking level (only when
// the connector's caps.thinkingLevel is true: Claude Code, connectors/src/lib.rs takes_thinking_level),
// Enabled. The open Connector or Model list is the combobox popover (`ComboboxList`), drawn in place of the
// app's portal. `enter` (0 → 1) is the dialog-in / overlay-in motion; `saving` is isPending (Saving…,
// both buttons disabled:opacity-60).
//
// Render as a child of <AgentsPage> (it positions against the window).

const LABEL = "mb-1 block font-body text-sm font-medium text-bark-800";

const Input: React.FC<{ value: string }> = ({ value }) => (
  <div
    className="w-full rounded-md border px-3 py-2 font-body text-sm leading-5 text-text-primary"
    style={FIELD}
  >
    {value}
  </div>
);

export type OpenList = {
  options: readonly string[];
  highlight: string;
  searchPlaceholder?: string;
  side?: "bottom" | "top";
};

export const EditAgentDialog: React.FC<{
  name: string;
  role: string;
  responsibilities: readonly string[];
  systemPrompt: readonly string[];
  /** The Connector trigger: the current option's label, "{name} — {status}". */
  connectorLabel: string;
  /** The Model trigger label: the model's name, or "{connector}'s default". */
  modelLabel: string;
  /** Thinking level's current option ("{connector}'s default" for no level), shown only for a connector
   * that takes one. */
  thinkingLevelPlaceholder?: string;
  connectorList?: OpenList;
  modelList?: OpenList;
  scrollTop?: number;
  savePressed?: boolean;
  saving?: boolean;
  enter?: number;
}> = ({
  name,
  role,
  responsibilities,
  systemPrompt,
  connectorLabel,
  modelLabel,
  thinkingLevelPlaceholder,
  connectorList,
  modelList,
  scrollTop = 0,
  savePressed = false,
  saving = false,
  enter = 1,
}) => (
  <div className="absolute inset-0 z-50">
    <div className="absolute inset-0 bg-overlay" style={{ opacity: enter }} />
    <div
      className="absolute inset-0 m-auto flex h-fit max-h-[648px] w-[768px] flex-col overflow-hidden rounded-xl border border-border bg-paper-50 p-6 shadow-lg"
      style={{ opacity: enter, translate: `0px ${(1 - enter) * 8}px` }}
    >
      {/* The dialog's top padding: scrolled fields pass under it, as under the scroll box's edge. */}
      <div className="absolute inset-x-0 top-0 z-10 h-6 bg-paper-50" />
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="min-h-0 flex-1">
          <div className="space-y-4" style={{ marginTop: -scrollTop }}>
            <div>
              <h2 className="font-display text-xl font-semibold text-bark-900">{AGENTS.editTitle}</h2>
              <p className="mt-1 font-body text-sm text-text-secondary">{AGENTS.editDescription(name)}</p>
            </div>

            <div>
              <p className={LABEL}>{AGENTS.name}</p>
              <Input value={name} />
            </div>

            <div>
              <p className={LABEL}>{AGENTS.role}</p>
              <Input value={role} />
            </div>

            <div>
              <p className={LABEL}>
                {AGENTS.skills}
                <span className="ml-1 font-normal text-text-muted">{AGENTS.skillsHint}</span>
              </p>
              <p className="font-body text-sm text-text-muted">
                {AGENTS.noPlugins[0]}{" "}
                <span className="font-medium text-moss-700">{AGENTS.noPlugins[1]}</span> {AGENTS.noPlugins[2]}
              </p>
              <p className="mt-2 font-body text-xs text-text-muted">{AGENTS.pluginsNextRun}</p>
            </div>

            <div>
              <p className={LABEL}>
                {AGENTS.responsibilities}
                <span className="ml-1 font-normal text-text-muted">{AGENTS.onePerLine}</span>
              </p>
              {/* rows={3}: three 20 px lines, py-2 and the border. */}
              <div
                className="h-[78px] w-full overflow-hidden whitespace-pre rounded-md border px-3 py-2 font-body text-sm leading-5 text-text-primary"
                style={FIELD}
              >
                {responsibilities.join("\n")}
              </div>
            </div>

            <div>
              <p className={LABEL}>{AGENTS.systemPrompt}</p>
              {/* rows={20} at leading-relaxed (22.75 px), py-2 and the border. */}
              <div
                className="h-[473px] w-full overflow-hidden whitespace-pre-wrap rounded-md border px-3 py-2 font-mono text-sm leading-relaxed text-text-primary"
                style={FIELD}
              >
                {systemPrompt.join("\n")}
              </div>
            </div>

            <div>
              <p className={LABEL}>{AGENTS.connector}</p>
              <div className="relative">
                <ComboboxTrigger label={connectorLabel} />
                {connectorList && (
                  <ComboboxList
                    options={connectorList.options}
                    value={connectorLabel}
                    highlight={connectorList.highlight}
                    searchPlaceholder={connectorList.searchPlaceholder}
                    side={connectorList.side}
                  />
                )}
              </div>
            </div>

            <div>
              <div className="mb-1 flex items-baseline justify-between gap-3">
                <p className="font-body text-sm font-medium text-bark-800">{AGENTS.model}</p>
                <p className="font-body text-xs font-medium text-moss-700">{AGENTS.refreshModels}</p>
              </div>
              <div className="relative">
                <ComboboxTrigger label={modelLabel} />
                {modelList && (
                  <ComboboxList
                    options={modelList.options}
                    value={modelLabel}
                    highlight={modelList.highlight}
                    searchPlaceholder={modelList.searchPlaceholder}
                    side={modelList.side}
                  />
                )}
              </div>
            </div>

            {thinkingLevelPlaceholder && (
              <div>
                <p className={LABEL}>{AGENTS.thinkingLevel}</p>
                <ComboboxTrigger label={thinkingLevelPlaceholder} />
              </div>
            )}

            <div className="flex items-center gap-2">
              {/* A native checkbox: the app sets no accent-color and has no forms plugin, so Chrome draws
                  its own blue box (info is the nearest token). */}
              <div className="flex h-4 w-4 items-center justify-center rounded-sm bg-info">
                <Check className="h-3 w-3 text-background" strokeWidth={3} aria-hidden />
              </div>
              <span className="font-body text-sm text-text-primary">{AGENTS.enabled}</span>
            </div>
          </div>
        </div>

        <div className="relative z-10 -mx-6 -mb-6 mt-6 flex shrink-0 justify-end gap-2 border-t border-border bg-paper-50 px-6 py-4">
          <div
            className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary"
            style={saving ? { opacity: 0.6 } : undefined}
          >
            {AGENTS.cancel}
          </div>
          <div
            className={`rounded-md px-4 py-2 font-body text-sm font-medium text-paper-50 ${
              savePressed ? "bg-moss-700" : "bg-moss-600"
            }`}
            style={saving ? { opacity: 0.6 } : undefined}
          >
            {saving ? AGENTS.saving : AGENTS.saveChanges}
          </div>
        </div>
      </div>
    </div>
  </div>
);
