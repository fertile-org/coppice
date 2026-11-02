import { useEffect, useRef, useState } from 'react';
import { useConfirm } from '../../components/ui/confirm-dialog';
import { Switch } from '../../components/ui/switch';
import type { Plugin } from '../../lib/schemas/plugin';
import { parseApiErrorMessage } from '../../lib/api';
import { cn } from '../../lib/utils';
import { pluginAgentSummary } from './copy';
import { confirmStart } from './pluginConsent';
import { rowSecondaryText, STATUS_HINTS, statusPillClass } from './installedPlugins';
import { useSetPluginEnabled } from './usePlugins';

const TONE_CLASS = {
  danger: 'text-danger',
  warning: 'text-warning',
  muted: 'text-text-secondary',
} as const;

interface PluginRowProps {
  plugin: Plugin;
  selected: boolean;
  highlighted: boolean;
  actionError: string | null;
  onOpen: () => void;
  onButtonRef?: (el: HTMLButtonElement | null) => void;
}

export function PluginRow({
  plugin,
  selected,
  highlighted,
  actionError,
  onOpen,
  onButtonRef,
}: PluginRowProps) {
  const [enableError, setEnableError] = useState<string | null>(null);
  const rowRef = useRef<HTMLLIElement>(null);
  const setEnabled = useSetPluginEnabled();
  const confirm = useConfirm();
  const isExternal = plugin.status === 'external';
  const canToggle = plugin.enabled || plugin.status === 'ok';
  const line = rowSecondaryText(plugin, enableError ?? actionError);

  useEffect(() => {
    if (highlighted) rowRef.current?.scrollIntoView?.({ behavior: 'smooth', block: 'center' });
  }, [highlighted]);

  async function handleToggle() {
    setEnableError(null);
    if (!(await confirmStart(plugin, confirm))) return;
    try {
      await setEnabled.mutateAsync({ id: plugin.id, enabled: !plugin.enabled });
    } catch (err) {
      setEnableError(parseApiErrorMessage(err, 'Unable to update plugin.'));
    }
  }

  return (
    <li
      ref={rowRef}
      data-testid={`plugin-row-${plugin.id}`}
      data-highlighted={highlighted || undefined}
      className={cn(
        'flex h-14 items-center gap-3 overflow-hidden border-t border-border px-3.5 first:border-t-0',
        selected && 'bg-moss-500/10',
        highlighted && 'ring-2 ring-inset ring-moss-500',
      )}
    >
      <button
        type="button"
        ref={onButtonRef}
        onClick={onOpen}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate font-display text-sm font-semibold text-text-primary">
              {plugin.name}
            </span>
            <span className="shrink-0 font-mono text-xs text-text-muted">{plugin.version}</span>
            {plugin.status !== 'ok' && (
              <span title={STATUS_HINTS[plugin.status]} className={statusPillClass(plugin.status)}>
                {plugin.status}
              </span>
            )}
          </span>
          {line && (
            <span
              role={line.alert ? 'alert' : undefined}
              className={cn('mt-0.5 block truncate font-body text-xs', TONE_CLASS[line.tone])}
            >
              {line.text}
            </span>
          )}
        </span>
        {!isExternal && (
          <span
            data-testid="plugin-agent-access-summary"
            className="shrink-0 text-right font-body text-xs text-text-muted"
          >
            {plugin.skills.length} {plugin.skills.length === 1 ? 'skill' : 'skills'}
            {' · '}
            {plugin.mcpServers.length}{' '}
            {plugin.mcpServers.length === 1 ? 'MCP server' : 'MCP servers'}
            {' · '}
            {pluginAgentSummary(plugin.agentAccess, plugin.agentIds.length)}
          </span>
        )}
      </button>
      {!isExternal && (
        <Switch
          checked={plugin.enabled}
          aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
          title={canToggle ? undefined : 'Only plugins with status ok can be enabled'}
          disabled={!canToggle || setEnabled.isPending}
          onCheckedChange={() => void handleToggle()}
        />
      )}
    </li>
  );
}
