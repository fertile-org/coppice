import { useState } from 'react';
import { MermaidDiagram } from '../../components/MermaidDiagram';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import type { DesignOption, DesignSet } from '../../lib/schemas/design';
import { DESIGN_COPY } from './designCopy';
import { useChooseDesign } from './useDesigns';

/** Side-by-side technical option cards with diagram, fields, and badges. */
export function TechnicalDesignPicker({
  design,
  ticketId,
  sectionId,
}: {
  design: DesignSet;
  ticketId: string;
  sectionId: string;
}) {
  const [expandId, setExpandId] = useState<string | null>(null);
  const choose = useChooseDesign(ticketId);
  const canPick = design.choice.mode === 'update_plan' || design.choice.mode === 'save_only';
  const expanded = design.options.find((option) => option.id === expandId) ?? null;

  return (
    <div
      id={sectionId}
      data-testid={`design-picker-${design.id}`}
      data-design-kind={design.kind}
      className="my-3 space-y-2"
    >
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
        {design.options.map((option) => {
          const picked = design.chosen?.id === option.id;
          const selected = design.selected?.id === option.id;
          const recommended = design.recommended?.id === option.id;
          return (
            <TechnicalOptionCard
              key={option.id}
              option={option}
              canPick={canPick}
              pending={choose.isPending}
              picked={picked}
              selected={selected}
              recommended={recommended}
              recommendedReason={
                recommended ? design.recommendedReason ?? null : null
              }
              onPick={() => {
                if (!canPick || choose.isPending) return;
                void choose.mutateAsync({ setId: design.id, optionId: option.id });
              }}
              onExpand={() => setExpandId(option.id)}
            />
          );
        })}
      </div>
      {design.choice.planOutOfDate ? (
        <p className="font-body text-xs text-danger">{DESIGN_COPY.planNotUpdated}</p>
      ) : null}
      {expanded ? (
        <Dialog open onOpenChange={(open) => (open ? undefined : setExpandId(null))}>
          <DialogContent
            showCloseButton
            aria-describedby={undefined}
            className="flex max-h-[90vh] w-[calc(100%-2rem)] max-w-3xl flex-col gap-3 overflow-hidden"
          >
            <DialogHeader>
              <DialogTitle className="text-base">
                {DESIGN_COPY.fullDiagram}
              </DialogTitle>
              <p className="font-body text-sm text-text-secondary">
                {DESIGN_COPY.optionName(expanded)}
              </p>
            </DialogHeader>
            <div className="min-h-0 flex-1 overflow-auto">
              {expanded.sketch?.trim() ? (
                <MermaidDiagram source={expanded.sketch} />
              ) : (
                <p className="font-body text-sm text-text-secondary">
                  {DESIGN_COPY.noSketch}
                </p>
              )}
            </div>
          </DialogContent>
        </Dialog>
      ) : null}
    </div>
  );
}

function TechnicalOptionCard({
  option,
  canPick,
  pending,
  picked,
  selected,
  recommended,
  recommendedReason,
  onPick,
  onExpand,
}: {
  option: DesignOption;
  canPick: boolean;
  pending: boolean;
  picked: boolean;
  selected: boolean;
  recommended: boolean;
  recommendedReason: string | null;
  onPick: () => void;
  onExpand: () => void;
}) {
  return (
    <div
      role={canPick ? 'button' : undefined}
      tabIndex={canPick ? 0 : undefined}
      data-testid={`technical-option-${option.id}`}
      onClick={() => {
        if (!canPick || pending) return;
        onPick();
      }}
      onKeyDown={(event) => {
        if (!canPick || pending) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onPick();
        }
      }}
      className={[
        'flex flex-col gap-2 rounded-md border bg-surface p-3',
        recommended ? 'border-moss-600' : 'border-border',
        selected ? 'outline outline-2 outline-offset-1 outline-accent' : '',
        canPick ? 'cursor-pointer hover:bg-surface-raised' : '',
      ]
        .filter(Boolean)
        .join(' ')}
    >
      <div className="flex flex-wrap gap-1.5">
        {recommended ? (
          <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
            {DESIGN_COPY.recommended}
          </span>
        ) : null}
        {picked ? (
          <span className="rounded-full bg-moss-50 px-2 py-0.5 font-body text-xs text-moss-800">
            {DESIGN_COPY.yourPick}
          </span>
        ) : null}
      </div>
      <p className="font-body text-sm font-medium text-text-primary">
        {DESIGN_COPY.optionName(option)}
      </p>
      <div className="relative overflow-hidden rounded-md border border-border bg-paper-200">
        <button
          type="button"
          aria-label={DESIGN_COPY.expandDiagram}
          className="absolute right-1.5 top-1.5 z-10 rounded border border-border bg-surface px-1.5 py-0.5 font-body text-xs text-text-secondary hover:bg-surface-raised"
          onClick={(event) => {
            event.stopPropagation();
            onExpand();
          }}
        >
          ⤢
        </button>
        <div className="max-h-36 overflow-hidden p-2 [&_.mb-3]:mb-0">
          {option.sketch?.trim() ? (
            <MermaidDiagram source={option.sketch} />
          ) : (
            <p className="font-body text-xs text-text-secondary">{DESIGN_COPY.noSketch}</p>
          )}
        </div>
      </div>
      <dl className="grid grid-cols-[5.5rem_1fr] gap-x-2 gap-y-1 font-body text-xs">
        <dt className="text-text-muted">{DESIGN_COPY.fieldChanges}</dt>
        <dd className="text-text-primary">{option.changes ?? '—'}</dd>
        <dt className="text-text-muted">{DESIGN_COPY.fieldEffort}</dt>
        <dd className="text-text-primary">{option.effort ?? '—'}</dd>
        <dt className="text-text-muted">{DESIGN_COPY.fieldRisk}</dt>
        <dd className="text-text-primary">{option.risk ?? '—'}</dd>
        <dt className="text-text-muted">{DESIGN_COPY.fieldReversibility}</dt>
        <dd className="text-text-primary">{option.reversibility ?? '—'}</dd>
        <dt className="text-text-muted">{DESIGN_COPY.fieldRulesOut}</dt>
        <dd className="text-text-primary">{option.rulesOut ?? '—'}</dd>
      </dl>
      {recommendedReason ? (
        <p className="font-body text-xs text-moss-800">
          {DESIGN_COPY.whyRecommended}: {recommendedReason}
        </p>
      ) : null}
    </div>
  );
}
