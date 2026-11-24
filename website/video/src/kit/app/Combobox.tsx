import { Check, ChevronsUpDown, Search } from "lucide-react";
import React from "react";

// Mirrors web/src/components/ui/combobox.tsx with its popover.tsx and command.tsx parts.
// `ComboboxTrigger` is the closed control (the `field-control` class inlined as token styles;
// `focused` is its focus-visible ring, `muted` the placeholder colour). `ComboboxList` is the open
// popover: PopoverContent (p-0, trigger width, 4 px side offset), CommandInput only when there are
// more than 5 options (searchable 'auto'), CommandList (max-h-72), one CommandGroup (p-1), items
// with the highlighted one in bg-accent-muted and a Check on the current value. The list is drawn
// in place, absolutely positioned against the trigger's wrapper, instead of in a portal.

export const FIELD = {
  borderColor: "var(--color-field-border)",
  backgroundColor: "var(--color-field-bg)",
  boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.06)",
} as const;

export const FIELD_FOCUSED = {
  borderColor: "var(--color-accent)",
  backgroundColor: "var(--color-field-bg)",
  boxShadow: "inset 0 1px 2px rgba(0, 0, 0, 0.04), 0 0 0 2px var(--color-accent-muted)",
} as const;

export const ComboboxTrigger: React.FC<{ label: string; focused?: boolean; muted?: boolean }> = ({
  label,
  focused = false,
  muted = false,
}) => (
  <div
    className="flex h-10 w-full items-center justify-between gap-2 rounded-md border px-3 py-2 font-body text-sm text-text-primary"
    style={focused ? FIELD_FOCUSED : FIELD}
  >
    <span className={`min-w-0 truncate ${muted ? "text-text-muted" : ""}`}>{label}</span>
    <ChevronsUpDown className="size-4 shrink-0 text-text-muted" aria-hidden />
  </div>
);

const AUTO_SEARCH_THRESHOLD = 5;

export const ComboboxList: React.FC<{
  options: readonly string[];
  /** The current value (Check). */
  value: string;
  /** The item under the pointer or keyboard (bg-accent-muted). */
  highlight: string;
  searchPlaceholder?: string;
  /** Radix flips the popover above the trigger when it doesn't fit below. */
  side?: "bottom" | "top";
}> = ({ options, value, highlight, searchPlaceholder = "Search…", side = "bottom" }) => (
  <div
    className={`absolute inset-x-0 z-[100] overflow-hidden rounded-md border border-border bg-surface-raised p-0 text-text-primary shadow-lg ${
      side === "bottom" ? "top-[calc(100%+4px)]" : "bottom-[calc(100%+4px)]"
    }`}
  >
    {options.length > AUTO_SEARCH_THRESHOLD && (
      <div className="flex items-center gap-2 border-b border-border px-3">
        <Search className="size-4 shrink-0 text-text-muted" aria-hidden />
        <span className="flex h-9 w-full items-center py-2 font-body text-sm text-text-muted">
          {searchPlaceholder}
        </span>
      </div>
    )}
    <div className="max-h-72 overflow-hidden">
      <div className="p-1">
        {options.map((option) => (
          <div
            key={option}
            className={`relative flex items-center gap-2 rounded-sm px-2 py-2 font-body text-sm ${
              option === highlight ? "bg-accent-muted text-text-primary" : ""
            }`}
          >
            <span className="min-w-0 flex-1 truncate">{option}</span>
            <Check
              aria-hidden
              className="ml-auto size-4 shrink-0 text-accent"
              style={{ opacity: option === value ? 1 : 0 }}
            />
          </div>
        ))}
      </div>
    </div>
  </div>
);
