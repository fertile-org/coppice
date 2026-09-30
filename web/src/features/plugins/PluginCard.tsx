import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Button } from '../../components/ui/button';
import type { Plugin, PluginStatus } from '../../lib/schemas/plugin';
import { cn } from '../../lib/utils';
import { parseApiErrorMessage } from '../../lib/api';
import { usePluginInstall, useSetPluginEnabled, useUpdatePlugin } from './usePlugins';

function statusPillClass(status: PluginStatus): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  switch (status) {
    case 'ok':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'shadowed':
      return `${base} border-info-muted bg-info-muted text-info`;
    case 'missing':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
    case 'invalid':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
  }
}

interface PluginCardProps {
  plugin: Plugin;
}

export function PluginCard({ plugin }: PluginCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [updateId, setUpdateId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const setEnabled = useSetPluginEnabled();
  const updatePlugin = useUpdatePlugin();
  const { data: update } = usePluginInstall(updateId);
  const canToggle = plugin.enabled || plugin.status === 'ok';
  const updating = updatePlugin.isPending || update?.status === 'running';

  async function handleToggle() {
    setError(null);
    try {
      await setEnabled.mutateAsync({ id: plugin.id, enabled: !plugin.enabled });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to update plugin.'));
    }
  }

  async function handleUpdate() {
    setError(null);
    try {
      const started = await updatePlugin.mutateAsync(plugin.id);
      setUpdateId(started.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to start update.'));
    }
  }

  return (
    <div
      data-testid={`plugin-card-${plugin.id}`}
      className="rounded-xl border border-border bg-surface-raised p-4 shadow-card"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="font-display text-base font-semibold text-text-primary">
              {plugin.name}
            </h3>
            <span className="font-mono text-xs text-text-muted">{plugin.version}</span>
            <span className={statusPillClass(plugin.status)}>{plugin.status}</span>
          </div>
          {plugin.description && (
            <p className="mt-1 font-body text-sm text-text-secondary">
              {plugin.description}
            </p>
          )}
          <p className="mt-1 flex flex-wrap items-center gap-x-2 font-body text-xs text-text-muted">
            <span>{plugin.source === 'git' ? 'Git' : 'Local'}</span>
            {plugin.gitCommit && (
              <span className="font-mono" title={plugin.gitCommit}>
                {plugin.gitCommit.slice(0, 7)}
              </span>
            )}
            <span>·</span>
            <span>
              {plugin.skills.length} {plugin.skills.length === 1 ? 'skill' : 'skills'}
            </span>
            <span>·</span>
            <span>
              {plugin.mcpServers.length}{' '}
              {plugin.mcpServers.length === 1 ? 'MCP server' : 'MCP servers'}
            </span>
          </p>
          {plugin.error && (
            <p className="mt-1 font-body text-xs text-danger">{plugin.error}</p>
          )}
          {plugin.unsupported.length > 0 && (
            <p className="mt-1 font-body text-xs text-warning">
              {`Not supported yet: ${plugin.unsupported.join(', ')}`}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {plugin.source === 'git' && (
            <Button
              type="button"
              variant="secondary"
              onClick={() => void handleUpdate()}
              loading={updating}
            >
              {updating ? 'Updating…' : 'Update'}
            </Button>
          )}
          <button
            type="button"
            role="switch"
            aria-checked={plugin.enabled}
            aria-label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
            title={canToggle ? undefined : 'Only plugins with status ok can be enabled'}
            disabled={!canToggle || setEnabled.isPending}
            onClick={() => void handleToggle()}
            className={cn(
              'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-border transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-50',
              plugin.enabled ? 'bg-accent' : 'bg-paper-200',
            )}
          >
            <span
              aria-hidden
              className={cn(
                'inline-block h-3.5 w-3.5 rounded-full bg-surface-raised shadow-card transition-transform duration-fast',
                plugin.enabled ? 'translate-x-4' : 'translate-x-0.5',
              )}
            />
          </button>
        </div>
      </div>

      {update?.status === 'failed' && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          Update failed: {update.error ?? 'unknown error'}
        </p>
      )}
      {error && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          {error}
        </p>
      )}

      {plugin.skills.length > 0 && (
        <div className="mt-3">
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded((prev) => !prev)}
            className="inline-flex items-center gap-1 font-body text-xs font-medium text-text-secondary hover:text-text-primary"
          >
            {expanded ? (
              <ChevronDown className="h-3.5 w-3.5" aria-hidden />
            ) : (
              <ChevronRight className="h-3.5 w-3.5" aria-hidden />
            )}
            Skills
          </button>
          {expanded && (
            <ul className="mt-2 space-y-2 border-t border-border pt-2">
              {plugin.skills.map((skill) => (
                <li key={skill.relPath}>
                  <p className="font-mono text-xs text-text-primary">{skill.name}</p>
                  {skill.description && (
                    <p className="font-body text-xs text-text-secondary">
                      {skill.description}
                    </p>
                  )}
                  {skill.error && (
                    <p className="font-body text-xs text-danger">{skill.error}</p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
