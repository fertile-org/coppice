import { useState } from 'react';
import { Button } from '../../components/ui/button';
import type { DesignSet } from '../../lib/schemas/design';
import { DesignDialog } from './DesignDialog';
import { DESIGN_COPY } from './designCopy';
import type { DesignScope } from './useDesigns';

export function DesignCard({ design, scope }: { design: DesignSet; scope?: DesignScope }) {
  const [open, setOpen] = useState(false);

  return (
    <section className="rounded-md border border-border bg-surface-raised px-3 py-3">
      <div className="flex items-start justify-between gap-2">
        <h3 className="font-body text-sm font-medium text-text-primary">{design.title}</h3>
        {design.chosen ? (
          <span className="inline-flex shrink-0 items-center rounded-full border border-border px-2 py-0.5 font-body text-xs text-text-secondary">
            {DESIGN_COPY.chosen}
          </span>
        ) : null}
      </div>
      <ul className="mt-1 flex flex-wrap gap-x-3 gap-y-1">
        {design.options.map((option) => (
          <li key={option.id} className="font-body text-xs text-text-secondary">
            {DESIGN_COPY.optionName(option)}
          </li>
        ))}
      </ul>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        className="mt-2"
        onClick={() => setOpen(true)}
      >
        {DESIGN_COPY.open}
      </Button>
      {open ? (
        <DesignDialog design={design} scope={scope} onClose={() => setOpen(false)} />
      ) : null}
    </section>
  );
}
