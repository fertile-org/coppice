import { useState } from 'react';
import { Button } from '../../components/ui/button';
import { DesignDialog } from './DesignDialog';
import { DESIGN_COPY } from './designCopy';
import { useDesigns } from './useDesigns';

/** Opens the design options the plan was written around, above the plan. */
export function DesignPlanCard({ ticketId }: { ticketId: string }) {
  const { data } = useDesigns(ticketId);
  const [open, setOpen] = useState(false);
  const live = (data ?? []).filter((set) => !set.superseded);
  const design = live.find((set) => set.fromPlanRun) ?? live[0];
  if (!design) return null;

  const { selected } = design;
  const detail = selected
    ? `${DESIGN_COPY.options(design.options.length)} · ${DESIGN_COPY.using(
        selected.id,
        selected.title,
        design.chosen !== null,
      )}`
    : DESIGN_COPY.options(design.options.length);

  return (
    <section className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-accent bg-surface-raised px-4 py-3">
      <div>
        <h3 className="font-body text-sm font-medium text-text-primary">{DESIGN_COPY.cardTitle}</h3>
        <p className="font-body text-xs text-text-secondary">{detail}</p>
      </div>
      <Button type="button" size="sm" onClick={() => setOpen(true)}>
        {DESIGN_COPY.open}
      </Button>
      {open ? (
        <DesignDialog design={design} scope={ticketId} onClose={() => setOpen(false)} />
      ) : null}
    </section>
  );
}
