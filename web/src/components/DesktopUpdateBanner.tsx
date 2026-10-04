import { Download, X } from 'lucide-react';
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

export function DesktopUpdateBanner() {
  const [update, setUpdate] = useState<DesktopUpdateInfo | null>(null);

  useEffect(() => {
    const getUpdateInfo = window.coppiceDesktop?.getUpdateInfo;
    if (!getUpdateInfo) {
      return;
    }
    let cancelled = false;
    getUpdateInfo()
      .then((info) => {
        if (!cancelled && info && info.version !== readDismissedVersion()) {
          setUpdate(info);
        }
      })
      .catch(() => {
        /* update checks are best-effort */
      });
    return () => {
      cancelled = true;
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

  return (
    <div
      role="status"
      data-testid="desktop-update-banner"
      className="flex shrink-0 items-center gap-3 border-b border-moss-500/30 bg-moss-500/10 px-4 py-2 sm:px-6"
    >
      <span className="font-body text-sm text-text-primary">
        Coppice {update.version} is available
      </span>
      <a
        href={update.url}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1.5 font-body text-sm font-medium text-accent hover:underline"
      >
        <Download className="h-4 w-4 shrink-0" aria-hidden />
        Download
      </a>
      <button
        type="button"
        onClick={dismiss}
        aria-label="Dismiss update notice"
        className="ml-auto rounded-md p-1 text-text-secondary transition-colors duration-fast hover:bg-paper-200 hover:text-text-primary"
      >
        <X className="h-4 w-4" aria-hidden />
      </button>
    </div>
  );
}
