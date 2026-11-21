import type { DesignSet } from '../../lib/schemas/design';
import { DESIGN_COPY } from './designCopy';
import { livePlanDesigns } from './useDesigns';

export function pickerDomId(setId: string): string {
  return `design-picker-${setId}`;
}

/** One-line summary of every live plan-run set; each entry scrolls to its picker. */
export function DesignPlanSummary({
  sets,
  planMarkdown = '',
  sections,
}: {
  sets: DesignSet[];
  planMarkdown?: string;
  /** Precomputed section labels keyed by set id (from markers / fallbacks). */
  sections?: Record<string, string>;
}) {
  const live = livePlanDesigns(sets);
  if (live.length === 0) return null;

  function labelFor(set: DesignSet): string {
    return sections?.[set.id]
      ?? sectionFromPlan(planMarkdown, set.id)
      ?? set.title;
  }

  function pickLabel(set: DesignSet): string {
    if (set.chosen) {
      return DESIGN_COPY.optionName(set.chosen);
    }
    return DESIGN_COPY.notPicked;
  }

  function jump(setId: string) {
    const el = document.getElementById(pickerDomId(setId));
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  return (
    <p
      data-testid="design-plan-summary"
      className="rounded-md border border-border bg-surface px-3 py-2 font-body text-sm text-text-secondary"
    >
      <span className="font-medium text-text-primary">
        {DESIGN_COPY.decisionsInPlan(live.length)}
      </span>
      {live.map((set) => (
        <span key={set.id}>
          {' · '}
          <button
            type="button"
            className="text-accent underline-offset-2 hover:underline"
            onClick={() => jump(set.id)}
          >
            {DESIGN_COPY.sectionPick(labelFor(set), pickLabel(set))}
          </button>
        </span>
      ))}
    </p>
  );
}

function sectionFromPlan(markdown: string, setId: string): string | null {
  const needle = `{{design:${setId}}}`;
  const pos = markdown.indexOf(needle);
  if (pos < 0) return null;
  let heading: string | null = null;
  for (const line of markdown.slice(0, pos).split('\n')) {
    const trimmed = line.trim();
    if (trimmed.startsWith('## ') && !trimmed.startsWith('###')) {
      heading = trimmed.slice(3).trim();
    }
  }
  return heading;
}

/** @deprecated Prefer DesignPlanSummary; kept name for import sites during the swap. */
export function DesignPlanCard({
  sets,
  planMarkdown,
}: {
  ticketId?: string;
  sets?: DesignSet[];
  planMarkdown?: string;
}) {
  if (!sets) return null;
  return <DesignPlanSummary sets={sets} planMarkdown={planMarkdown} />;
}
