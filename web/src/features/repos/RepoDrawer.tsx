import { useEffect, type ReactNode } from 'react';

export interface RepoDrawerProps {
  ariaLabel: string;
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
}

/** Form-scale right drawer (TicketDrawer mechanics, narrower panel). */
export function RepoDrawer({
  ariaLabel,
  title,
  description,
  onClose,
  children,
}: RepoDrawerProps) {
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex justify-end" role="presentation">
      <div
        data-testid="repo-drawer-backdrop"
        className="absolute inset-0 bg-bark-950/40 backdrop-blur-[1px]"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        className="relative flex h-full w-full max-w-lg animate-fade-in flex-col bg-surface-raised shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-6 py-4">
          <div className="min-w-0">
            <h2 className="font-display text-xl font-semibold text-bark-900">
              {title}
            </h2>
            {description && (
              <p className="mt-1 font-body text-sm text-text-secondary">
                {description}
              </p>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="shrink-0 rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            Close
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{children}</div>
      </div>
    </div>
  );
}
