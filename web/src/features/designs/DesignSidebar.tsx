import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/button';
import type { ChosenDesignOption, DesignOption, DesignSet } from '../../lib/schemas/design';
import { RepoDrawer } from '../repos/RepoDrawer';
import { rewriteDesignHtml } from './rewriteDesignHtml';

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
    return <p className="font-body text-sm text-text-secondary">This preview has no file.</p>;
  }
  if (!url) {
    return <p className="font-body text-sm text-text-muted">Loading preview…</p>;
  }
  return (
    <iframe
      title={option.title}
      sandbox="allow-scripts"
      src={url}
      className="h-[70vh] w-full rounded-md border border-border bg-surface"
    />
  );
}

export function DesignSidebar({
  design,
  chosen,
  pending,
  error,
  onClose,
  onChoose,
}: {
  design: DesignSet;
  chosen: ChosenDesignOption | null;
  pending: boolean;
  error: string | null;
  onClose: () => void;
  onChoose: (optionId: string) => void;
}) {
  const [activeId, setActiveId] = useState(design.options[0]?.id ?? '');
  const active = design.options.find((option) => option.id === activeId) ?? design.options[0];

  return (
    <RepoDrawer open ariaLabel={design.title} title={design.title} onClose={onClose}>
      {active ? (
        <>
          <div role="tablist" aria-label="Design options" className="flex gap-1 border-b border-border">
            {design.options.map((option) => (
              <button
                key={option.id}
                type="button"
                role="tab"
                aria-selected={option.id === active.id}
                onClick={() => setActiveId(option.id)}
                className={[
                  'border-b-2 px-3 py-2 font-body text-sm transition-colors duration-fast',
                  option.id === active.id
                    ? 'border-accent text-accent'
                    : 'border-transparent text-text-secondary hover:text-text-primary',
                ].join(' ')}
              >
                {option.title}
              </button>
            ))}
          </div>
          <div role="tabpanel" aria-label={active.title} className="mt-3">
            <DesignPreview key={active.id} setId={design.id} option={active} />
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <Button type="button" onClick={() => onChoose(active.id)} disabled={pending}>
              Choose
            </Button>
            {chosen?.id === active.id ? (
              <span className="inline-flex items-center rounded-full border border-border px-2 py-0.5 font-body text-xs text-text-secondary">
                Chosen
              </span>
            ) : null}
            {error ? <p className="font-body text-xs text-danger">{error}</p> : null}
          </div>
        </>
      ) : (
        <p className="font-body text-sm text-text-secondary">This preview has no file.</p>
      )}
    </RepoDrawer>
  );
}
