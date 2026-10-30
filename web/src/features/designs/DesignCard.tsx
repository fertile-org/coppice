import { useState } from 'react';
import { Button } from '../../components/ui/button';
import type { ChosenDesignOption, DesignSet } from '../../lib/schemas/design';
import { DesignSidebar } from './DesignSidebar';
import { useChooseDesign, type DesignScope } from './useDesigns';

export function DesignCard({ design, scope }: { design: DesignSet; scope?: DesignScope }) {
  const choose = useChooseDesign(scope);
  const [open, setOpen] = useState(false);
  const [picked, setPicked] = useState<ChosenDesignOption | null>(null);
  const [error, setError] = useState<string | null>(null);
  const chosen = picked ?? design.chosen;

  async function pick(optionId: string) {
    setError(null);
    try {
      await choose.mutateAsync({ setId: design.id, optionId });
      const option = design.options.find((item) => item.id === optionId);
      if (option) setPicked({ id: option.id, title: option.title });
    } catch {
      setError('Unable to save that choice.');
    }
  }

  return (
    <section className="rounded-md border border-border bg-surface-raised px-3 py-3">
      <div className="flex items-start justify-between gap-2">
        <h3 className="font-body text-sm font-medium text-text-primary">{design.title}</h3>
        {chosen ? (
          <span className="inline-flex shrink-0 items-center rounded-full border border-border px-2 py-0.5 font-body text-xs text-text-secondary">
            Chosen
          </span>
        ) : null}
      </div>
      <ul className="mt-1 flex flex-wrap gap-x-3 gap-y-1">
        {design.options.map((option) => (
          <li key={option.id} className="font-body text-xs text-text-secondary">
            {option.title}
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
        Open
      </Button>
      {open ? (
        <DesignSidebar
          design={design}
          chosen={chosen}
          pending={choose.isPending}
          error={error}
          onClose={() => setOpen(false)}
          onChoose={(optionId) => {
            void pick(optionId);
          }}
        />
      ) : null}
    </section>
  );
}
