import { useEffect, useState } from 'react';
import type { DesktopAppInfo, DesktopUpdateCheck } from '../../lib/desktop';
import { Button } from '../../components/ui/button';
import { Switch } from '../../components/ui/switch';

export function updateHelperText(info: Pick<DesktopAppInfo, 'platform' | 'macInPlaceUpdates'>): string {
  if (info.platform === 'darwin' && info.macInPlaceUpdates) {
    return 'Download new versions in the background. They install when you restart Coppice.';
  }
  if (info.platform === 'linux') {
    return 'Tell me when a new version is out. Updates install through apt.';
  }
  return 'Tell me when a new version is out.';
}

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
  const [helper, setHelper] = useState<string | null>(null);
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
        setHelper(updateHelperText(info));
        setAutomatic(settings.automaticUpdates);
        setReady(true);
      })
      .catch(() => {
        /* The page shows nothing to change. */
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, supported]);

  if (!supported || !ready || version == null || helper == null) return null;

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
    <section aria-label="Updates" className="space-y-3">
      <p className="font-body text-sm text-text-primary">Version {version}</p>
      <div className="flex flex-wrap items-center gap-3">
        <Switch
          checked={automatic}
          aria-label="Automatic updates"
          onCheckedChange={() => void toggle()}
        />
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
