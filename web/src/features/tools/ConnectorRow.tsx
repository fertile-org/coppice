import { useState } from 'react';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '../../components/ui/dialog';
import { Switch } from '../../components/ui/switch';
import { parseApiErrorMessage } from '../../lib/api';
import type {
  ConnectorCheck,
  ConnectorStatus,
} from '../../lib/schemas/connectorDiagnostics';
import { cn } from '../../lib/utils';
import { turnOffConfirm } from './connectorCopy';
import { formatRelativeTime } from './connectorFormat';
import {
  agentCountLabel,
  lastRunMeta,
  rowSecondaryText,
} from './connectorList';
import { useSetConnectorEnabled } from './useConnectorDiagnostics';

const TONE_CLASS = {
  danger: 'text-danger',
  warning: 'text-warning',
  muted: 'text-text-secondary',
  info: 'text-info',
} as const;

interface ConnectorRowProps {
  connector: ConnectorStatus;
  selected: boolean;
  agentCount: number | null;
  liveCheck: ConnectorCheck | undefined;
  checkError: boolean;
  hasLiveCheck: boolean;
  onOpen: () => void;
  onButtonRef?: (el: HTMLButtonElement | null) => void;
  onToggleError?: (error: string | null) => void;
}

export function ConnectorRow({
  connector,
  selected,
  agentCount,
  liveCheck,
  checkError,
  hasLiveCheck,
  onOpen,
  onButtonRef,
  onToggleError,
}: ConnectorRowProps) {
  const setEnabled = useSetConnectorEnabled();
  const [toggleError, setToggleError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const countKnown = agentCount !== null;
  const line = rowSecondaryText(
    connector,
    toggleError,
    liveCheck,
    checkError,
    hasLiveCheck,
  );

  function setError(message: string | null) {
    setToggleError(message);
    onToggleError?.(message);
  }

  async function applyEnabled(enabled: boolean) {
    setError(null);
    setConfirming(false);
    try {
      await setEnabled.mutateAsync({ id: connector.id, enabled });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not update this connector.'));
    }
  }

  function handleToggle() {
    if (connector.enabled) {
      if (!countKnown) return;
      if ((agentCount ?? 0) > 0) {
        setConfirming(true);
        return;
      }
      void applyEnabled(false);
      return;
    }
    void applyEnabled(true);
  }

  return (
    <li
      data-testid={`connector-row-${connector.id}`}
      className={cn(
        'flex h-14 items-center gap-3 overflow-hidden border-t border-border px-3.5 first:border-t-0',
        selected ? 'bg-moss-500/10' : 'hover:bg-moss-500/5',
      )}
    >
      <button
        type="button"
        ref={onButtonRef}
        onClick={onOpen}
        className="keyboard-focus flex min-w-0 flex-1 items-center gap-3 rounded-sm text-left"
      >
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate font-display text-sm font-semibold text-text-primary">
              {connector.displayName}
            </span>
            <span className="shrink-0 font-mono text-xs text-text-muted">
              {connector.id}
            </span>
          </span>
          {line && (
            <span
              role={line.alert ? 'alert' : undefined}
              title={line.text}
              className={cn('mt-0.5 block truncate font-body text-xs', TONE_CLASS[line.tone])}
            >
              {line.text}
            </span>
          )}
        </span>
        <span className="hidden shrink-0 text-right font-body text-xs text-text-muted sm:block">
          {agentCountLabel(agentCount)}
          {' · '}
          {lastRunMeta(connector, formatRelativeTime)}
        </span>
      </button>
      <Switch
        checked={connector.enabled}
        aria-label={`Enabled ${connector.displayName}`}
        disabled={setEnabled.isPending || (connector.enabled && !countKnown)}
        onCheckedChange={handleToggle}
      />
      <Dialog open={confirming} onOpenChange={(open) => !open && setConfirming(false)}>
        <DialogContent>
          <DialogTitle className="sr-only">Turn off {connector.displayName}</DialogTitle>
          <p className="font-body text-sm text-text-primary">
            {turnOffConfirm(connector.displayName, agentCount ?? 0)}
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button type="button" variant="destructive" onClick={() => void applyEnabled(false)}>
              Turn off
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </li>
  );
}
