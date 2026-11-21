import { useState } from 'react';
import { Button } from '../../components/ui/button';
import type { DesignSet } from '../../lib/schemas/design';
import { DesignDialog } from './DesignDialog';
import { DESIGN_COPY } from './designCopy';
import { useChooseDesign } from './useDesigns';

/** Radio-style option rows for a design set embedded in the plan. */
export function InlineDesignPicker({
  design,
  ticketId,
  sectionId,
}: {
  design: DesignSet;
  ticketId: string;
  /** DOM id for summary scroll targets. */
  sectionId: string;
}) {
  const [openId, setOpenId] = useState<string | null>(null);
  const choose = useChooseDesign(ticketId);
  const canPick = design.choice.mode === 'update_plan' || design.choice.mode === 'save_only';

  return (
    <div
      id={sectionId}
      data-testid={`design-picker-${design.id}`}
      className="my-3 overflow-hidden rounded-md border border-border"
    >
      {design.options.map((option) => {
        const picked = design.chosen?.id === option.id;
        const selected = design.selected?.id === option.id;
        const recommended = design.recommended?.id === option.id;
        return (
          <div
            key={option.id}
            role={canPick ? 'button' : undefined}
            tabIndex={canPick ? 0 : undefined}
            onClick={() => {
              if (!canPick || choose.isPending) return;
              void choose.mutateAsync({ setId: design.id, optionId: option.id });
            }}
            onKeyDown={(event) => {
              if (!canPick || choose.isPending) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                void choose.mutateAsync({ setId: design.id, optionId: option.id });
              }
            }}
            className={`flex items-center gap-2.5 border-t border-border px-3 py-2 first:border-t-0 ${
              selected ? 'bg-moss-50' : 'hover:bg-surface-raised'
            } ${canPick ? 'cursor-pointer' : ''}`}
          >
            <span
              aria-hidden
              className={`h-3.5 w-3.5 shrink-0 rounded-full border-2 ${
                selected ? 'border-moss-600 bg-moss-600/30' : 'border-border'
              }`}
            />
            <div className="min-w-0 flex-1 font-body text-sm text-text-primary">
              <span className="font-medium">
                {DESIGN_COPY.optionName(option)}
              </span>
              {recommended ? (
                <span className="ml-2 rounded bg-surface-raised px-1.5 py-0.5 text-xs text-text-secondary">
                  {DESIGN_COPY.recommended}
                </span>
              ) : null}
              {picked ? (
                <span className="ml-2 rounded bg-moss-100 px-1.5 py-0.5 text-xs text-moss-800">
                  {DESIGN_COPY.yourPick}
                </span>
              ) : null}
            </div>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              onClick={(event) => {
                event.stopPropagation();
                setOpenId(option.id);
              }}
            >
              {DESIGN_COPY.open}
            </Button>
          </div>
        );
      })}
      {design.choice.planOutOfDate ? (
        <p className="border-t border-border px-3 py-2 font-body text-xs text-danger">
          {DESIGN_COPY.planNotUpdated}
        </p>
      ) : null}
      {openId ? (
        <DesignDialog
          design={design}
          scope={ticketId}
          initialOptionId={openId}
          onClose={() => setOpenId(null)}
        />
      ) : null}
    </div>
  );
}
