import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import type {
  Plugin,
  PluginMcpServerHealth,
  PluginStatus,
} from '../../lib/schemas/plugin';
import { cn } from '../../lib/utils';
import { parseApiErrorMessage } from '../../lib/api';
import { PluginSettingsForm } from './PluginSettingsForm';
import { PluginTestResults } from './PluginTestResults';
import {
  usePluginInstall,
  useSetPluginEnabled,
  useSetSkillEnabled,
  useTestPlugin,
  useUpdatePlugin,
} from './usePlugins';

const STDIO_RISK =
  "Local MCP servers run on your computer with your computer's privileges. Stronger sandboxing is coming in a later release.";

/** Names keys only; the values never reach the browser. */
function envRisk(keys: string[]): string {
  const list = keys.join(', ');
  return keys.length === 1
    ? `Setting ${list} is not set here, so your computer's environment value for it will be sent to the plugin.`
    : `Settings ${list} are not set here, so your computer's environment values for them will be sent to the plugin.`;
}

/** Starting a disabled plugin's servers (enable or Test) needs the admin's consent. */
function confirmStart(plugin: Plugin): boolean {
  if (plugin.enabled) return true;
  const envKeys = plugin.settings.filter((s) => s.source === 'env').map((s) => s.key);
  const risks = [
    plugin.mcpServers.some((server) => server.kind === 'stdio') ? STDIO_RISK : null,
    envKeys.length > 0 ? envRisk(envKeys) : null,
  ].filter((risk): risk is string => risk !== null);
  return risks.length === 0 || window.confirm(`${risks.join(' ')} Enable anyway?`);
}

const STATUS_HINTS: Record<PluginStatus, string> = {
  ok: 'Loaded and ready to enable.',
  shadowed:
    'Another plugin with the same name in an earlier plugin directory is used instead.',
  missing: 'The folder is gone from disk. Agent assignments are kept until it returns.',
  invalid: 'The plugin could not be read. See the error below.',
  external: 'Lives in another repository; install it separately.',
};

const HEALTH_HINTS: Record<PluginMcpServerHealth, string> = {
  stopped: 'Not running. Starts when an agent run or Test needs it.',
  starting: 'Starting up.',
  ready: 'Running; its tools are available to agents.',
  backoff: 'Failed recently; Coppice will retry shortly.',
  unhealthy: 'Failed repeatedly; its tools are hidden. Fix the cause, then press Test.',
};

function healthClass(health: PluginMcpServerHealth): string {
  switch (health) {
    case 'ready':
      return 'text-success';
    case 'starting':
      return 'text-info';
    case 'backoff':
      return 'text-warning';
    case 'unhealthy':
      return 'text-danger';
    case 'stopped':
      return 'text-text-muted';
  }
}

function McpServersSection({ plugin }: { plugin: Plugin }) {
  const [error, setError] = useState<string | null>(null);
  const testPlugin = useTestPlugin(plugin.id);

  async function handleTest() {
    if (!confirmStart(plugin)) return;
    setError(null);
    testPlugin.reset();
    try {
      await testPlugin.mutateAsync();
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Test failed.'));
    }
  }

  return (
    <div className="mt-3 border-t border-border pt-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="font-body text-xs font-medium text-text-secondary">MCP servers</p>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          loading={testPlugin.isPending}
          onClick={() => void handleTest()}
        >
          Test
        </Button>
      </div>
      <ul className="mt-2 space-y-1">
        {plugin.mcpServers.map((server) => (
          <li
            key={server.name}
            data-testid={`plugin-mcp-server-${server.name}`}
            className="flex items-center gap-2"
          >
            <span className="font-mono text-xs text-text-primary">{server.name}</span>
            <span className="font-body text-xs text-text-muted">{server.kind}</span>
            <span
              title={HEALTH_HINTS[server.health]}
              className={cn('font-body text-xs', healthClass(server.health))}
            >
              {server.health}
            </span>
          </li>
        ))}
      </ul>
      <p className="mt-1 font-body text-xs text-text-muted">
        {`Agents call these tools as ${plugin.name}__<tool>.`}
      </p>
      {testPlugin.data && <PluginTestResults result={testPlugin.data} />}
      {error && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          {error}
        </p>
      )}
    </div>
  );
}

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
    case 'external':
      return `${base} border-border bg-paper-200 text-text-secondary`;
  }
}

interface SwitchProps {
  checked: boolean;
  label: string;
  title?: string;
  disabled?: boolean;
  onToggle: () => void;
}

function Switch({ checked, label, title, disabled, onToggle }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={title}
      disabled={disabled}
      onClick={onToggle}
      className={cn(
        'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-border transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-50',
        checked ? 'bg-accent' : 'bg-paper-200',
      )}
    >
      <span
        aria-hidden
        className={cn(
          'inline-block h-3.5 w-3.5 rounded-full bg-surface-raised shadow-card transition-transform duration-fast',
          checked ? 'translate-x-4' : 'translate-x-0.5',
        )}
      />
    </button>
  );
}

function skillsHeader(skills: Plugin['skills']): string {
  const on = skills.filter((skill) => skill.enabled).length;
  return on === skills.length
    ? `Skills (${skills.length})`
    : `Skills (${on} of ${skills.length} on)`;
}

function SkillsSection({ plugin }: { plugin: Plugin }) {
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setSkillEnabled = useSetSkillEnabled(plugin.id);

  async function handleToggle(skill: string, enabled: boolean) {
    setError(null);
    try {
      await setSkillEnabled.mutateAsync({ skill, enabled });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to update skill.'));
    }
  }

  return (
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
        {skillsHeader(plugin.skills)}
      </button>
      {expanded && (
        <div className="mt-2 border-t border-border pt-2">
          <p className="font-body text-xs text-text-muted">
            An agent sees each skill’s name and description, and loads one when it
            needs it.
          </p>
          <ul className="mt-2 space-y-2">
            {plugin.skills.map((skill) => (
              <li key={skill.relPath} className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="font-mono text-xs text-text-primary">{skill.name}</p>
                  {skill.description && (
                    <p className="font-body text-xs text-text-secondary">
                      {skill.description}
                    </p>
                  )}
                  {skill.error && (
                    <p className="font-body text-xs text-danger">{skill.error}</p>
                  )}
                </div>
                <Switch
                  checked={skill.enabled}
                  label={`${skill.enabled ? 'Disable' : 'Enable'} ${skill.name}`}
                  disabled={setSkillEnabled.isPending}
                  onToggle={() => void handleToggle(skill.name, !skill.enabled)}
                />
              </li>
            ))}
          </ul>
          {error && (
            <p role="alert" className="mt-2 font-body text-xs text-danger">
              {error}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function ExternalSource({
  external,
  onInstallFromGit,
}: {
  external: NonNullable<Plugin['external']>;
  onInstallFromGit?: (url: string) => void;
}) {
  const { url } = external;
  return (
    <div className="mt-2 flex flex-wrap items-center gap-2">
      <p className="font-body text-xs text-text-secondary">
        {url ? `Lives in another repository: ${url}` : `Source: ${external.kind}`}
      </p>
      {url && (
        <Button
          type="button"
          variant="secondary"
          size="sm"
          onClick={() => onInstallFromGit?.(url)}
        >
          Install from git
        </Button>
      )}
    </div>
  );
}

interface PluginCardProps {
  plugin: Plugin;
  onInstallFromGit?: (url: string) => void;
  /** Plugins sharing this plugin's git clone, itself included. */
  siblingCount?: number;
}

export function PluginCard({ plugin, onInstallFromGit, siblingCount = 1 }: PluginCardProps) {
  const [updateId, setUpdateId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const setEnabled = useSetPluginEnabled();
  const updatePlugin = useUpdatePlugin();
  const { data: update } = usePluginInstall(updateId);
  const isExternal = plugin.status === 'external';
  const canToggle = plugin.enabled || plugin.status === 'ok';
  const updating = updatePlugin.isPending || update?.status === 'running';

  async function handleToggle() {
    setError(null);
    if (!confirmStart(plugin)) return;
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
            <span
              title={STATUS_HINTS[plugin.status]}
              className={statusPillClass(plugin.status)}
            >
              {plugin.status}
            </span>
          </div>
          {plugin.description && (
            <p className="mt-1 font-body text-sm text-text-secondary">
              {plugin.description}
            </p>
          )}
          {plugin.marketplace && (
            <p className="mt-1 font-body text-xs text-text-muted">
              {`From marketplace ${plugin.marketplace.name}`}
            </p>
          )}
          <p className="mt-1 flex flex-wrap items-center gap-x-2 font-body text-xs text-text-muted">
            <span>{plugin.source === 'git' ? 'Git' : 'Local'}</span>
            {plugin.gitCommit && (
              <span className="font-mono" title={plugin.gitCommit}>
                {plugin.gitCommit.slice(0, 7)}
              </span>
            )}
            {!isExternal && (
              <>
                <span>·</span>
                <span>
                  {plugin.skills.length} {plugin.skills.length === 1 ? 'skill' : 'skills'}
                </span>
                <span>·</span>
                <span>
                  {plugin.mcpServers.length}{' '}
                  {plugin.mcpServers.length === 1 ? 'MCP server' : 'MCP servers'}
                </span>
              </>
            )}
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
        {!isExternal && (
          <div className="flex shrink-0 items-center gap-2">
            {plugin.source === 'git' && (
              <Button
                type="button"
                variant="secondary"
                title={
                  siblingCount > 1
                    ? `Updates all ${siblingCount} plugins from this repository`
                    : undefined
                }
                onClick={() => void handleUpdate()}
                loading={updating}
              >
                {updating ? 'Updating…' : 'Update'}
              </Button>
            )}
            <Switch
              checked={plugin.enabled}
              label={`${plugin.enabled ? 'Disable' : 'Enable'} ${plugin.name}`}
              title={canToggle ? undefined : 'Only plugins with status ok can be enabled'}
              disabled={!canToggle || setEnabled.isPending}
              onToggle={() => void handleToggle()}
            />
          </div>
        )}
      </div>

      {plugin.external && (
        <ExternalSource external={plugin.external} onInstallFromGit={onInstallFromGit} />
      )}

      {plugin.enabled && plugin.status === 'ok' && (
        <p className="mt-2 font-body text-xs text-text-secondary">
          <Link
            to={`/agents?plugin=${plugin.id}`}
            className="font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Give it to an agent →
          </Link>{' '}
          <span className="text-text-muted">
            {`Opens Agents with ${plugin.name} pre-selected.`}
          </span>
        </p>
      )}

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

      {!isExternal && plugin.mcpServers.length > 0 && (
        <McpServersSection plugin={plugin} />
      )}
      {!isExternal && plugin.settings.length > 0 && (
        <PluginSettingsForm pluginId={plugin.id} settings={plugin.settings} />
      )}
      {!isExternal && plugin.skills.length > 0 && <SkillsSection plugin={plugin} />}
    </div>
  );
}
