import type { ReactNode } from 'react';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetTitle,
} from '../../components/ui/sheet';

export interface RepoDrawerProps {
  /** Keep the drawer mounted and toggle this so the exit animation plays. */
  open?: boolean;
  ariaLabel: string;
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
}

/** Form-scale right drawer (TicketDrawer mechanics, narrower panel). */
export function RepoDrawer({
  open = true,
  ariaLabel,
  title,
  description,
  onClose,
  children,
}: RepoDrawerProps) {
  return (
    <Sheet open={open} onOpenChange={(next) => !next && onClose()}>
      <SheetContent
        aria-label={ariaLabel}
        aria-labelledby={undefined}
        {...(description ? {} : { 'aria-describedby': undefined })}
        className="max-w-lg"
        overlayProps={{ 'data-testid': 'repo-drawer-backdrop' }}
      >
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-6 py-4">
          <div className="min-w-0">
            <SheetTitle>{title}</SheetTitle>
            {description && (
              <SheetDescription className="mt-1">{description}</SheetDescription>
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
      </SheetContent>
    </Sheet>
  );
}
