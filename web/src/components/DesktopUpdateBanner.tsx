import { useEffect, useState } from 'react';
import type { DesktopUpdateInfo } from '../lib/desktop';

const DISMISSED_UPDATE_KEY = 'coppice.dismissedUpdate';

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

export function DesktopUpdateBanner() {
  const [update, setUpdate] = useState<DesktopUpdateInfo | null>(null);

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
        ) : update.kind === 'linux' ? (
          <>
            Coppice {update.version} is available. Install it from Software Updater, or run{' '}
            <code className="font-mono text-[0.95em]">sudo apt update && sudo apt upgrade</code>.
          </>
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
    </div>
  );
}
