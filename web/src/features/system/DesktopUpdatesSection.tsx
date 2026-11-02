import { Check } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { LINUX_GUIDE_TEXT, LinuxUpdateGuide, runIsActive } from '../../components/LinuxUpdateGuide';
import { Button } from '../../components/ui/button';
import { Switch } from '../../components/ui/switch';
import type { DesktopAppInfo, DesktopUpdateCheck, DesktopUpdateInfo } from '../../lib/desktop';

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

const RUN_BLOCKED_TEXT = 'Wait for the agent run to finish, then try again.';

function UpdateGuidePanel({
  info,
  platform,
  runBlocked,
  onRestart,
  onDownload,
}: {
  info: DesktopUpdateInfo;
  platform: string;
  runBlocked: boolean;
  onRestart: () => void;
  onDownload: (url: string) => void;
}) {
  const linuxAvailable = info.kind === 'linux';
  const linuxReady = info.kind === 'ready' && platform === 'linux';
  const macReady = info.kind === 'ready' && platform === 'darwin';
  const macDownload = info.kind === 'download';
  if (!linuxAvailable && !linuxReady && !macReady && !macDownload) {
    return null;
  }

  return (
    <section className="max-w-xl rounded-md border border-border bg-surface p-4">
      <h2 className="font-display text-base font-semibold text-text-primary">
        Update to Coppice {info.version}
      </h2>
      {linuxAvailable || linuxReady ? (
        <>
          <p className="mt-1 font-body text-sm text-text-secondary">{LINUX_GUIDE_TEXT}</p>
          <LinuxUpdateGuide />
        </>
      ) : null}
      {linuxReady ? (
        <>
          <p className="mt-3 flex items-center gap-2 font-body text-sm text-text-primary">
            <Check className="size-4 shrink-0 text-accent" aria-hidden="true" />
            Coppice {info.version} is installed. Restart to start using it. Your boards and
            settings stay.
          </p>
          <Button type="button" className="mt-3" onClick={onRestart}>
            Restart Coppice
          </Button>
        </>
      ) : null}
      {macReady ? (
        <Button type="button" className="mt-3" onClick={onRestart}>
          Restart to update
        </Button>
      ) : null}
      {macDownload ? (
        <Button type="button" className="mt-3" onClick={() => onDownload(info.url)}>
          Download update
        </Button>
      ) : null}
      {runBlocked ? (
        <p className="mt-3 font-body text-sm text-text-secondary">{RUN_BLOCKED_TEXT}</p>
      ) : null}
    </section>
  );
}

export function DesktopUpdatesSection() {
  const desktop = typeof window !== 'undefined' ? window.coppiceDesktop : undefined;
  const supported = Boolean(desktop?.appInfo && desktop.getUpdateSettings && desktop.checkForUpdates);
  const [version, setVersion] = useState<string | null>(null);
  const [platform, setPlatform] = useState<string | null>(null);
  const [helper, setHelper] = useState<string | null>(null);
  const [automatic, setAutomatic] = useState(true);
  const [ready, setReady] = useState(false);
  const [check, setCheck] = useState<CheckState>({ kind: 'idle' });
  const [update, setUpdate] = useState<DesktopUpdateInfo | null>(null);
  const [runBlocked, setRunBlocked] = useState(false);
  const lastInfoRef = useRef<DesktopUpdateInfo | null>(null);

  useEffect(() => {
    if (!supported || !desktop?.appInfo || !desktop.getUpdateSettings) return;
    let cancelled = false;
    Promise.all([desktop.appInfo(), desktop.getUpdateSettings()])
      .then(([info, settings]) => {
        if (cancelled) return;
        setVersion(info.version);
        setPlatform(info.platform);
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

  useEffect(() => {
    const getUpdateInfo = desktop?.getUpdateInfo;
    const onUpdateInfo = desktop?.onUpdateInfo;
    if (!getUpdateInfo && !onUpdateInfo) return;
    let cancelled = false;
    let seenBroadcast = false;
    const apply = (info: DesktopUpdateInfo | null) => {
      if (cancelled) return;
      const prev = lastInfoRef.current;
      lastInfoRef.current = info;
      setUpdate(info);
      setRunBlocked(false);
      setCheck((current) => {
        if (current.kind === 'checking') return current;
        if (info == null) {
          if (prev != null && current.kind === 'available') return { kind: 'idle' };
          return current;
        }
        if (info.kind === 'ready') {
          return current.kind === 'available' ? { kind: 'idle' } : current;
        }
        return { kind: 'available', version: info.version };
      });
    };
    const unsubscribe = onUpdateInfo?.((info) => {
      seenBroadcast = true;
      apply(info);
    });
    getUpdateInfo?.()
      .then((info) => {
        if (seenBroadcast) return;
        apply(info);
      })
      .catch(() => {
        /* update checks are best-effort */
      });
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [desktop]);

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
      setCheck(() => {
        if (lastInfoRef.current?.kind === 'ready') return { kind: 'idle' };
        if (result.status === 'available') return { kind: 'available', version: result.version };
        if (result.status === 'up-to-date') return { kind: 'up-to-date' };
        return { kind: 'error' };
      });
    } catch {
      setCheck(() => (lastInfoRef.current?.kind === 'ready' ? { kind: 'idle' } : { kind: 'error' }));
    }
  }

  function restart() {
    void runIsActive().then((active) => {
      if (active) {
        setRunBlocked(true);
        return;
      }
      setRunBlocked(false);
      void desktop?.restartToUpdate?.();
    });
  }

  function download(url: string) {
    void runIsActive().then((active) => {
      if (active) {
        setRunBlocked(true);
        return;
      }
      setRunBlocked(false);
      window.open(url, '_blank', 'noopener,noreferrer');
    });
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
      {update ? (
        <UpdateGuidePanel
          info={update}
          platform={platform ?? ''}
          runBlocked={runBlocked}
          onRestart={restart}
          onDownload={download}
        />
      ) : null}
    </section>
  );
}
