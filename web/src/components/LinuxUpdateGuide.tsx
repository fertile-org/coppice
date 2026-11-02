import { Check, Copy } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from './ui/button';

/** Matches `deb.packageName` in desktop/electron-builder.yml. */
export const LINUX_UPGRADE_COMMAND =
  'sudo apt update && sudo apt install --only-upgrade coppice-desktop';

export const LINUX_GUIDE_TEXT =
  'Open Software Updater and install the Coppice update, or run this command in a terminal. It updates only Coppice.';

export async function runIsActive(): Promise<boolean> {
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

export function LinuxUpdateGuide() {
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
  );
}
