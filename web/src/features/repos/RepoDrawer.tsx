import { useState, type ReactNode } from 'react';
import { Switch } from '../../components/ui/switch';
import type { Repo } from '../../lib/schemas/repo';
import { useUpdateRepo } from './useRepos';
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
  /** Same as TicketDrawer: drawers with no SheetTrigger return focus here. */
  onRestoreFocus?: () => void;
  children: ReactNode;
}

/** Form-scale right drawer (TicketDrawer mechanics, narrower panel). */
export function RepoDrawer({
  open = true,
  ariaLabel,
  title,
  description,
  onClose,
  onRestoreFocus,
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
        onCloseAutoFocus={(event) => {
          if (!onRestoreFocus) return;
          event.preventDefault();
          onRestoreFocus();
        }}
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

interface GitFlowSwitchProps {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  disabledReason?: string | null;
  onChange: (next: boolean) => void;
}

function GitFlowSwitch({
  id,
  label,
  description,
  checked,
  disabled,
  disabledReason,
  onChange,
}: GitFlowSwitchProps) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <label htmlFor={id} className="block font-body text-sm font-medium text-bark-800">
          {label}
        </label>
        <p className="mt-0.5 font-body text-sm text-text-secondary">{description}</p>
        {disabledReason && (
          <p className="mt-0.5 font-body text-sm text-text-secondary">{disabledReason}</p>
        )}
      </div>
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        className="mt-0.5"
      />
    </div>
  );
}

/** Per-repo switches for what happens once a ticket is accepted. */
export function RepoGitFlowSection({ repo }: { repo: Repo }) {
  const updateRepo = useUpdateRepo();
  const [error, setError] = useState<string | null>(null);

  async function save(patch: { mergeAfterAccept?: boolean; pushAfterMerge?: boolean }) {
    setError(null);
    try {
      await updateRepo.mutateAsync({ id: repo.id, ...patch });
    } catch {
      setError('Unable to save this setting. Please try again.');
    }
  }

  const branch = repo.defaultBranch;
  // A blocked push can still be switched off; only turning it on is gated.
  const pushDisabled = Boolean(repo.pushAfterMergeDisabledReason) && !repo.pushAfterMerge;

  return (
    <section className="space-y-4 border-t border-border pt-5">
      <h3 className="font-display text-sm font-semibold text-bark-800">
        When a ticket is accepted
      </h3>
      <GitFlowSwitch
        id="repo-merge-after-accept"
        label="Merge after accept"
        description={`Merge the ticket branch into ${branch} when you accept the ticket.`}
        checked={repo.mergeAfterAccept}
        disabled={updateRepo.isPending}
        onChange={(next) => void save({ mergeAfterAccept: next })}
      />
      <GitFlowSwitch
        id="repo-push-after-merge"
        label="Push after merge"
        description={`Push ${branch} after any ticket merge into it. This pushes the whole branch, including other local commits that aren't pushed yet.`}
        checked={repo.pushAfterMerge}
        disabled={pushDisabled || updateRepo.isPending}
        disabledReason={repo.pushAfterMergeDisabledReason}
        onChange={(next) => void save({ pushAfterMerge: next })}
      />
      {error && (
        <p
          role="alert"
          className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}
    </section>
  );
}
