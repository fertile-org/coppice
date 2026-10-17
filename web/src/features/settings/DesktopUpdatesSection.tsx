import { useEffect, useState } from 'react';
import type { DesktopUpdateCheck } from '../../lib/desktop';
import { Button } from '../../components/ui/button';

type CheckState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up-to-date' }
  | { kind: 'available'; version: string }
  | { kind: 'error' };

function checkText(state: CheckState): string | null {
  switch (state.kind) {
    case 'checking':
      return 'Checking…';
    case 'up-to-date':
      return "You're up to date.";
    case 'available':
      return `Coppice ${state.version} is available.`;
    case 'error':
      return "Couldn't check for updates. Try again later.";
    default:
      return null;
  }
}

export function DesktopUpdatesSection() {
  const desktop = typeof window !== 'undefined' ? window.coppiceDesktop : undefined;
  const supported = Boolean(desktop?.appInfo && desktop.getUpdateSettings && desktop.checkForUpdates);
  const [version, setVersion] = useState<string | null>(null);
  const [automatic, setAutomatic] = useState(true);
  const [ready, setReady] = useState(false);
  const [check, setCheck] = useState<CheckState>({ kind: 'idle' });

  useEffect(() => {
    if (!supported || !desktop?.appInfo || !desktop.getUpdateSettings) return;
    let cancelled = false;
    Promise.all([desktop.appInfo(), desktop.getUpdateSettings()])
      .then(([info, settings]) => {
        if (cancelled) return;
        setVersion(info.version);
        setAutomatic(settings.automaticUpdates);
        setReady(true);
      })
      .catch(() => {
        /* The rest of Settings still works. */
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, supported]);

  if (!supported || !ready || version == null) return null;

  const helper =
    desktop?.platform === 'darwin'
      ? 'Download new versions in the background. They install when you restart Coppice.'
      : 'Tell me when a new version is out. Updates install through apt.';

  async function toggle() {
    const next = !automatic;
    setAutomatic(next);
    try {
      const saved = await desktop?.setUpdateSettings?.({ automaticUpdates: next });
      if (saved) setAutomatic(saved.automaticUpdates);
    } catch {
      setAutomatic(!next);
    }
  }

  async function checkNow() {
    setCheck({ kind: 'checking' });
    try {
      const result: DesktopUpdateCheck = (await desktop?.checkForUpdates?.()) ?? { status: 'error' };
      if (result.status === 'available') setCheck({ kind: 'available', version: result.version });
      else if (result.status === 'up-to-date') setCheck({ kind: 'up-to-date' });
      else setCheck({ kind: 'error' });
    } catch {
      setCheck({ kind: 'error' });
    }
  }

  const status = checkText(check);

  return (
    <section aria-labelledby="desktop-updates-heading" className="space-y-3">
      <h2 id="desktop-updates-heading" className="font-display text-lg font-semibold text-text-primary">
        Updates
      </h2>
      <p className="font-body text-sm text-text-primary">Version {version}</p>
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          role="switch"
          aria-checked={automatic}
          aria-label="Automatic updates"
          onClick={() => void toggle()}
          className={
            automatic
              ? 'relative h-5 w-9 rounded-full border border-success-muted bg-success-muted'
              : 'relative h-5 w-9 rounded-full border border-border bg-paper-200'
          }
        >
          <span
            aria-hidden
            className={
              automatic
                ? 'absolute top-0.5 left-4 h-3.5 w-3.5 rounded-full bg-surface'
                : 'absolute top-0.5 left-0.5 h-3.5 w-3.5 rounded-full bg-surface'
            }
          />
        </button>
        <span className="font-body text-sm text-text-primary">Automatic updates</span>
      </div>
      <p className="font-body text-sm text-text-secondary">{helper}</p>
      <div className="flex flex-wrap items-center gap-3">
        <Button type="button" variant="secondary" onClick={() => void checkNow()} disabled={check.kind === 'checking'}>
          Check for updates
        </Button>
        {status ? (
          <p role="status" className="font-body text-sm text-text-primary">
            {status}
          </p>
        ) : null}
      </div>
    </section>
  );
}
