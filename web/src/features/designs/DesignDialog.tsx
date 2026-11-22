import { useEffect, useState } from 'react';
import { MermaidDiagram } from '../../components/MermaidDiagram';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import { parseApiErrorMessage } from '../../lib/api';
import type { DesignOption, DesignSet } from '../../lib/schemas/design';
import { DESIGN_COPY } from './designCopy';
import { rewriteDesignHtml } from './rewriteDesignHtml';
import { isTechnicalDesignKind } from './technicalKind';
import { useChooseDesign, type DesignScope } from './useDesigns';

function TechnicalDialogBody({
  option,
  recommendedReason,
}: {
  option: DesignOption;
  recommendedReason: string | null;
}) {
  return (
    <div className="space-y-3 pb-4">
      {option.sketch?.trim() ? (
        <MermaidDiagram source={option.sketch} />
      ) : (
        <p className="font-body text-sm text-text-secondary">{DESIGN_COPY.noSketch}</p>
      )}
      <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-1.5 font-body text-sm">
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
        <p className="font-body text-sm text-moss-800">
          {DESIGN_COPY.whyRecommended}: {recommendedReason}
        </p>
      ) : null}
    </div>
  );
}

function DesignPreview({ setId, option }: { setId: string; option: DesignOption }) {
  const [url, setUrl] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    let revoke = () => {};
    let cancelled = false;
    rewriteDesignHtml(setId, option.entry, controller.signal)
      .then((page) => {
        if (cancelled) {
          page.revoke();
          return;
        }
        revoke = page.revoke;
        setUrl(page.url);
      })
      .catch(() => {
        if (!cancelled) setMissing(true);
      });
    return () => {
      cancelled = true;
      controller.abort();
      revoke();
    };
  }, [setId, option.entry]);

  if (missing) {
    return <p className="font-body text-sm text-text-secondary">{DESIGN_COPY.noPreview}</p>;
  }
  if (!url) {
    return <p className="font-body text-sm text-text-muted">{DESIGN_COPY.loadingPreview}</p>;
  }
  return (
    <iframe
      title={DESIGN_COPY.optionName(option)}
      sandbox="allow-scripts"
      src={url}
      className="h-full min-h-56 w-full rounded-md border border-border bg-surface"
    />
  );
}

type Footer =
  | { kind: 'none' }
  | { kind: 'text'; text: string }
  | { kind: 'action'; text: string | null; label: string };

function footerFor(design: DesignSet, viewed: DesignOption): Footer {
  const { mode, planOutOfDate } = design.choice;
  const id = viewed.id;
  const isSelected = design.selected?.id === id;
  if (mode === 'read_only') return { kind: 'none' };
  if (mode === 'plan_final') return { kind: 'text', text: DESIGN_COPY.planFinal };
  if (mode === 'plan_running') return { kind: 'text', text: DESIGN_COPY.planRunning };
  if (isSelected && planOutOfDate) {
    return { kind: 'action', text: DESIGN_COPY.planNotUpdated, label: DESIGN_COPY.tryAgain };
  }
  // A work-run or chat set's recommendation drives nothing until someone confirms it.
  if (isSelected && (design.fromPlanRun || design.chosen?.id === id)) {
    return { kind: 'text', text: DESIGN_COPY.workFollows };
  }
  return {
    kind: 'action',
    text: null,
    label: mode === 'update_plan' ? DESIGN_COPY.chooseAndUpdate(viewed.label) : DESIGN_COPY.choose(viewed.label),
  };
}

export function DesignDialog({
  design,
  scope,
  initialOptionId,
  onClose,
}: {
  design: DesignSet;
  scope?: DesignScope;
  /** Open on this option when set (e.g. from an inline Open button). */
  initialOptionId?: string;
  onClose: () => void;
}) {
  const choose = useChooseDesign(scope);
  const [activeId, setActiveId] = useState(
    initialOptionId
      && design.options.some((option) => option.id === initialOptionId)
      ? initialOptionId
      : (design.selected?.id ?? design.options[0]?.id ?? ''),
  );
  const [error, setError] = useState<string | null>(null);
  const active = design.options.find((option) => option.id === activeId) ?? design.options[0];
  const footer = active ? footerFor(design, active) : ({ kind: 'none' } as const);

  async function pick(optionId: string) {
    setError(null);
    try {
      await choose.mutateAsync({ setId: design.id, optionId });
    } catch (err) {
      setError(parseApiErrorMessage(err, DESIGN_COPY.saveFailed));
    }
  }

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent
        showCloseButton
        aria-describedby={undefined}
        className="flex h-[92vh] w-[calc(100%-2rem)] max-w-[1600px] flex-col gap-3 overflow-hidden p-0"
      >
        <DialogHeader className="border-b border-border px-5 py-4">
          <DialogTitle className="text-base">{DESIGN_COPY.dialogTitle}</DialogTitle>
          <p className="font-body text-sm text-text-secondary">{design.title}</p>
        </DialogHeader>
        {active ? (
          <>
            <div
              role="tablist"
              aria-label="Design options"
              className="flex flex-wrap gap-2 px-5"
            >
              {design.options.map((option) => {
                const isActive = option.id === active.id;
                return (
                  <button
                    key={option.id}
                    type="button"
                    role="tab"
                    aria-selected={isActive}
                    onClick={() => {
                      setError(null);
                      setActiveId(option.id);
                    }}
                    className={[
                      'inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 font-body text-sm transition-colors duration-fast',
                      isActive
                        ? 'border-accent text-text-primary'
                        : 'border-border text-text-secondary hover:text-text-primary',
                    ].join(' ')}
                  >
                    {DESIGN_COPY.optionName(option)}
                    {design.recommended?.id === option.id ? (
                      <span className="rounded-full bg-accent px-1.5 text-xs font-medium text-white">
                        {DESIGN_COPY.recommended}
                      </span>
                    ) : null}
                    {design.chosen?.id === option.id ? (
                      <span className="rounded-full border border-border px-1.5 text-xs text-text-secondary">
                        {DESIGN_COPY.chosen}
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>
            <div
              role="tabpanel"
              aria-label={DESIGN_COPY.optionName(active)}
              className="min-h-0 flex-1 overflow-auto px-5"
            >
              {isTechnicalDesignKind(design.kind) ? (
                <TechnicalDialogBody
                  option={active}
                  recommendedReason={
                    design.recommended?.id === active.id
                      ? design.recommendedReason ?? null
                      : null
                  }
                />
              ) : (
                <DesignPreview key={active.id} setId={design.id} option={active} />
              )}
            </div>
          </>
        ) : (
          <p className="px-5 font-body text-sm text-text-secondary">{DESIGN_COPY.noPreview}</p>
        )}
        {footer.kind !== 'none' || error ? (
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border px-5 py-3">
            <div className="font-body text-sm text-text-secondary">
              {footer.kind === 'text' || footer.kind === 'action' ? footer.text : null}
              {error ? (
                <p role="alert" className="text-danger">
                  {error}
                </p>
              ) : null}
            </div>
            {footer.kind === 'action' && active ? (
              <Button
                type="button"
                onClick={() => {
                  void pick(active.id);
                }}
                disabled={choose.isPending}
              >
                {footer.label}
              </Button>
            ) : null}
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
