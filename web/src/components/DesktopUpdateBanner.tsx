import { Check, Copy } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { DesktopUpdateInfo } from '../lib/desktop';
import { Button } from './ui/button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';

const DISMISSED_UPDATE_KEY = 'coppice.dismissedUpdate';

/** Matches `deb.packageName` in desktop/electron-builder.yml. */
export const LINUX_UPGRADE_COMMAND =
  'sudo apt update && sudo apt install --only-upgrade coppice-desktop';

function readDismissedVersion(): string | null {
  try {
    return localStorage.getItem(DISMISSED_UPDATE_KEY);
  } catch {
    return null;
  }
}

async function runIsActive(): Promise<boolean> {
  try {
    const res = await fetch('/api/agent-runs/active', { credentials: 'include' });
    if (!res.ok) {
      return false;
    }
    const body = (await res.json()) as { active?: boolean };
    return body.active === true;
  } catch {
    return false;
  }
}

function LinuxUpdateDialog({
  version,
  installed,
  open,
  onOpenChange,
  onRestart,
}: {
  version: string;
  /** apt has put the new package on disk; a restart runs it. */
  installed: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRestart: () => void;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const copy = () => {
    void navigator.clipboard
      ?.writeText(LINUX_UPGRADE_COMMAND)
      .then(() => setCopied(true))
      .catch(() => {
        /* the command stays selectable */
      });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton className="max-w-xl">
        <DialogHeader>
          <DialogTitle>Update to Coppice {version}</DialogTitle>
          <DialogDescription>
            Open Software Updater and install the Coppice update, or run this command in a
            terminal. It updates only Coppice.
          </DialogDescription>
        </DialogHeader>
        <div className="mt-4 flex items-start gap-2 rounded-md border border-border bg-surface p-3">
          <pre className="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all font-mono text-sm text-text-primary">
            <code>
              <span className="select-none text-text-secondary">$ </span>
              {LINUX_UPGRADE_COMMAND}
            </code>
          </pre>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={copy}
            aria-label={copied ? 'Copied' : 'Copy command'}
            className="shrink-0"
          >
            {copied ? (
              <Check className="size-4" aria-hidden="true" />
            ) : (
              <Copy className="size-4" aria-hidden="true" />
            )}
          </Button>
        </div>
        {installed ? (
          <p className="mt-3 flex items-center gap-2 font-body text-sm text-text-primary">
            <Check className="size-4 shrink-0 text-accent" aria-hidden="true" />
            Coppice {version} is installed. Restart to start using it. Your boards and settings
            stay.
          </p>
        ) : (
          <p className="mt-3 font-body text-sm text-text-secondary">
            Once the update is installed, a Restart button appears here and in the banner.
          </p>
        )}
        <DialogFooter className="mt-6">
          {installed ? (
            <>
              <DialogClose asChild>
                <Button type="button" variant="ghost">
                  Later
                </Button>
              </DialogClose>
              <Button type="button" onClick={onRestart}>
                Restart Coppice
              </Button>
            </>
          ) : (
            <DialogClose asChild>
              <Button type="button">Done</Button>
            </DialogClose>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function DesktopUpdateBanner() {
  const [update, setUpdate] = useState<DesktopUpdateInfo | null>(null);
  const [linuxDialogOpen, setLinuxDialogOpen] = useState(false);

  useEffect(() => {
    const desktop = window.coppiceDesktop;
    const getUpdateInfo = desktop?.getUpdateInfo;
    if (!getUpdateInfo) {
      return;
    }
    let cancelled = false;
    const apply = (info: DesktopUpdateInfo | null) => {
      void (async () => {
        if (cancelled) return;
        if (!info) {
          setUpdate(null);
          return;
        }
        const active = await runIsActive();
        if (cancelled) return;
        if (active || info.version === readDismissedVersion()) {
          setUpdate(null);
          return;
        }
        setUpdate(info);
      })();
    };
    getUpdateInfo()
      .then(apply)
      .catch(() => {
        /* update checks are best-effort */
      });
    const unsubscribe = desktop?.onUpdateInfo?.(apply);
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, []);

  if (!update) {
    return null;
  }

  const dismiss = () => {
    try {
      localStorage.setItem(DISMISSED_UPDATE_KEY, update.version);
    } catch {
      /* ignore quota / private mode */
    }
    setUpdate(null);
  };

  const openRelease = () => {
    if (update.kind !== 'download') return;
    const url = update.url;
    void runIsActive().then((active) => {
      if (active) {
        setUpdate(null);
        return;
      }
      window.open(url, '_blank', 'noopener,noreferrer');
    });
  };

  const restart = () => {
    void runIsActive().then((active) => {
      if (active) {
        setUpdate(null);
        return;
      }
      void window.coppiceDesktop?.restartToUpdate?.();
    });
  };

  return (
    <div
      role="status"
      data-testid="desktop-update-banner"
      className="flex shrink-0 flex-wrap items-center gap-3 border-b border-moss-500/30 bg-moss-500/10 px-4 py-2 sm:px-6"
    >
      <span className="font-body text-sm text-text-primary">
        {update.kind === 'ready' ? (
          <>Coppice {update.version} is ready.</>
        ) : (
          <>Coppice {update.version} is available.</>
        )}
      </span>
      {update.kind === 'ready' ? (
        <button
          type="button"
          onClick={restart}
          className="font-body text-sm font-medium text-accent hover:underline"
        >
          Restart to update
        </button>
      ) : null}
      {update.kind === 'linux' ? (
        <button
          type="button"
          onClick={() => setLinuxDialogOpen(true)}
          className="font-body text-sm font-medium text-accent hover:underline"
        >
          How to update
        </button>
      ) : null}
      {update.kind === 'download' ? (
        <button
          type="button"
          onClick={openRelease}
          className="font-body text-sm font-medium text-accent hover:underline"
        >
          Download update
        </button>
      ) : null}
      <button
        type="button"
        onClick={dismiss}
        className="ml-auto font-body text-sm text-text-secondary hover:text-text-primary"
      >
        {update.kind === 'linux' ? 'Dismiss' : 'Later'}
      </button>
      {update.kind === 'linux' || (update.kind === 'ready' && linuxDialogOpen) ? (
        <LinuxUpdateDialog
          version={update.version}
          installed={update.kind === 'ready'}
          open={linuxDialogOpen}
          onOpenChange={setLinuxDialogOpen}
          onRestart={restart}
        />
      ) : null}
    </div>
  );
}
