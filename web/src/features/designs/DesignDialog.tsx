import { useEffect, useState } from 'react';
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
import { useChooseDesign, type DesignScope } from './useDesigns';

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
      title={option.title}
      sandbox="allow-scripts"
      src={url}
      className="h-[min(50vh,28rem)] min-h-56 w-full rounded-md border border-border bg-surface"
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
  if (isSelected) {
    return planOutOfDate
      ? { kind: 'action', text: DESIGN_COPY.planNotUpdated, label: DESIGN_COPY.tryAgain }
      : { kind: 'text', text: DESIGN_COPY.workFollows };
  }
  return {
    kind: 'action',
    text: null,
    label: mode === 'update_plan' ? DESIGN_COPY.chooseAndUpdate(id) : DESIGN_COPY.choose(id),
  };
}

export function DesignDialog({
  design,
  scope,
  onClose,
}: {
  design: DesignSet;
  scope?: DesignScope;
  onClose: () => void;
}) {
  const choose = useChooseDesign(scope);
  const [activeId, setActiveId] = useState(
    design.selected?.id ?? design.options[0]?.id ?? '',
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
        className="flex max-h-[90vh] max-w-4xl flex-col gap-3 overflow-y-auto p-0"
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
                    {option.title}
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
            <div role="tabpanel" aria-label={active.title} className="px-5">
              <DesignPreview key={active.id} setId={design.id} option={active} />
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
