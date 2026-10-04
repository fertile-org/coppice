import { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Combobox } from '../../components/ui/combobox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { ApiError, parseApiErrorMessage } from '../../lib/api';
import { isAssignable } from '../plugins/assignable';
import { usePlugins } from '../plugins/usePlugins';
import {
  AgentForm,
  agentToFormValues,
  listFromLines,
  presetToFormValues,
  type AgentFormValues,
  type PluginAssignmentState,
} from './AgentForm';
import {
  fetchAgentPlugins,
  useAgentPlugins,
  useAgentPresets,
  useAgents,
  useSetAgentPlugins,
  useConnectors,
  useCreateAgent,
  useUpdateAgent,
  useUpdateAgentMutation,
  type Agent,
  type AgentPreset,
  type ConnectorOption,
} from './useAgents';
import { KnowledgeCompactionCard } from './KnowledgeCompactionCard';
import { useKnowledgeSettings } from '../settings/useKnowledgeSettings';

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

function sameIds(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const set = new Set(a);
  return b.every((id) => set.has(id));
}

function HealthBadge({
  health,
  detail,
}: {
  health: Agent['health'];
  detail?: string | null;
}) {
  const labels = {
    unknown: { text: 'Unknown', className: 'bg-bark-100 text-bark-600' },
    healthy: { text: 'Healthy', className: 'bg-moss-100 text-moss-800' },
    missing_config: { text: 'Missing config', className: 'bg-amber-100 text-amber-900' },
    unhealthy: { text: 'Unhealthy', className: 'bg-danger-muted text-danger' },
  };
  const { text, className } = labels[health];
  return (
    <span
      title={detail ?? undefined}
      className={`inline-flex rounded-full px-2 py-0.5 font-body text-xs font-medium ${className}`}
    >
      {text}
    </span>
  );
}

function CreateAgentDialog({
  open,
  onClose,
  presets,
  connectorOptions,
}: {
  open: boolean;
  onClose: () => void;
  presets: AgentPreset[];
  connectorOptions: ConnectorOption[];
}) {
  const presetRef = useRef<HTMLButtonElement>(null);
  const [presetId, setPresetId] = useState('');
  const [values, setValues] = useState<AgentFormValues>(presetToFormValues(presets[0] ?? {
    id: '',
    key: '',
    role: '',
    skills: [],
    responsibilities: [],
    systemPromptTemplate: '',
  }));
  const [error, setError] = useState<string | null>(null);
  const [createdAgentId, setCreatedAgentId] = useState<string | null>(null);
  const createAgent = useCreateAgent();
  const setAgentPlugins = useSetAgentPlugins();

  useEffect(() => {
    if (!open) return;
    setCreatedAgentId(null);
    const first = presets[0];
    setPresetId(first?.id ?? '');
    setValues(presetToFormValues(first ?? {
      id: '',
      key: '',
      role: '',
      skills: [],
      responsibilities: [],
      systemPromptTemplate: '',
    }));
    setError(null);
    const timer = window.setTimeout(() => presetRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [open, presets]);

  function handlePresetChange(nextPresetId: string) {
    setPresetId(nextPresetId);
    const preset = presets.find((p) => p.id === nextPresetId);
    if (preset) {
      setValues((prev) => ({
        ...presetToFormValues(preset, prev.name),
        pluginIds: prev.pluginIds,
      }));
    }
  }

  async function assignPlugins(agentId: string, pluginIds: string[]) {
    if (pluginIds.length === 0) return;
    // Preset defaults were assigned server-side on create; add picks on top.
    const current = await fetchAgentPlugins(agentId);
    await setAgentPlugins.mutateAsync({
      agentId,
      pluginIds: [...new Set([...current, ...pluginIds])],
    });
  }

  async function handleSubmit(formValues: AgentFormValues) {
    setError(null);
    let agentId = createdAgentId;
    if (!agentId) {
      try {
        const agent = await createAgent.mutateAsync({
          name: formValues.name.trim(),
          presetId: presetId || undefined,
          systemPrompt: formValues.systemPrompt,
          connector: formValues.connector,
          modelProvider: formValues.modelProvider || undefined,
          model: formValues.model || undefined,
        });
        agentId = agent.id;
        setCreatedAgentId(agentId);
      } catch (err) {
        if (err instanceof ApiError && err.status === 400) {
          setError('Invalid agent configuration.');
        } else {
          setError('Unable to create agent. Please try again.');
        }
        return;
      }
    }
    try {
      await assignPlugins(agentId, formValues.pluginIds);
      onClose();
    } catch (err) {
      setError(
        `Agent created, but plugins could not be assigned: ${parseApiErrorMessage(
          err,
          'please try again.',
        )}`,
      );
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="max-h-[90vh] max-w-3xl overflow-y-auto">
        <DialogTitle>New agent</DialogTitle>
        <DialogDescription className="mt-1">
          Choose a preset to prefill role and prompt, then name your agent.
        </DialogDescription>

        <div className="mt-5">
          <label
            htmlFor="agent-preset"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Preset
          </label>
          <Combobox
            ref={presetRef}
            id="agent-preset"
            value={presetId}
            onValueChange={handlePresetChange}
            searchPlaceholder="Search presets…"
            options={presets.map((preset) => ({
              value: preset.id,
              label: `${preset.key} — ${preset.role}`,
            }))}
          />
        </div>

        <div className="mt-4">
          <AgentForm
            mode="create"
            values={values}
            onChange={setValues}
            onSubmit={handleSubmit}
            onCancel={onClose}
            connectorOptions={connectorOptions}
            isPending={createAgent.isPending || setAgentPlugins.isPending}
            error={error}
            submitLabel={createdAgentId ? 'Retry plugin assignment' : undefined}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function EditAgentDialog({
  open,
  agent,
  onClose,
  connectorOptions,
  preselectPluginId,
}: {
  open: boolean;
  agent: Agent;
  onClose: () => void;
  connectorOptions: ConnectorOption[];
  preselectPluginId?: string;
}) {
  const [values, setValues] = useState<AgentFormValues>(() =>
    agentToFormValues(agent),
  );
  const [error, setError] = useState<string | null>(null);
  const updateAgent = useUpdateAgent(agent.id);
  const {
    data: assignedPluginIds,
    error: assignedPluginsError,
    isSuccess: assignedPluginsLoaded,
  } = useAgentPlugins(agent.id);
  const setAgentPlugins = useSetAgentPlugins();
  const pluginAssignment: PluginAssignmentState = assignedPluginsLoaded
    ? { status: 'ready' }
    : assignedPluginsError
      ? {
          status: 'error',
          message: parseApiErrorMessage(assignedPluginsError, 'please try again.'),
        }
      : { status: 'loading' };

  const pluginsPrefilled = useRef(false);

  useEffect(() => {
    setValues((prev) => ({
      ...agentToFormValues(agent),
      pluginIds: prev.pluginIds,
    }));
    setError(null);
  }, [agent]);

  useEffect(() => {
    if (!assignedPluginIds || pluginsPrefilled.current) return;
    pluginsPrefilled.current = true;
    const pluginIds =
      preselectPluginId && !assignedPluginIds.includes(preselectPluginId)
        ? [...assignedPluginIds, preselectPluginId]
        : assignedPluginIds;
    setValues((prev) => ({ ...prev, pluginIds }));
  }, [assignedPluginIds, preselectPluginId]);

  async function handleSubmit(formValues: AgentFormValues) {
    setError(null);
    try {
      await updateAgent.mutateAsync({
        name: formValues.name.trim(),
        role: formValues.role.trim(),
        skills: listFromLines(formValues.skills),
        responsibilities: listFromLines(formValues.responsibilities),
        systemPrompt: formValues.systemPrompt,
        connector: formValues.connector,
        modelProvider: formValues.modelProvider || undefined,
        model: formValues.model || undefined,
        enabled: formValues.enabled,
      });
    } catch {
      setError('Unable to save agent.');
      return;
    }
    if (assignedPluginIds && !sameIds(assignedPluginIds, formValues.pluginIds)) {
      try {
        await setAgentPlugins.mutateAsync({
          agentId: agent.id,
          pluginIds: formValues.pluginIds,
        });
      } catch (err) {
        setError(
          `Agent saved, but plugins could not be updated: ${parseApiErrorMessage(
            err,
            'please try again.',
          )}`,
        );
        return;
      }
    }
    onClose();
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="max-h-[90vh] max-w-3xl overflow-y-auto">
        <DialogTitle>Edit agent</DialogTitle>
        <DialogDescription className="mt-1">
          Update {agent.name}&apos;s configuration.
        </DialogDescription>

        <div className="mt-5">
          <AgentForm
            mode="edit"
            values={values}
            onChange={setValues}
            onSubmit={handleSubmit}
            onCancel={onClose}
            connectorOptions={connectorOptions}
            isPending={updateAgent.isPending || setAgentPlugins.isPending}
            error={error}
            pluginAssignment={pluginAssignment}
            focusPlugins={Boolean(preselectPluginId)}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PluginDeepLinkBanner({
  pluginName,
  onDone,
}: {
  pluginName: string | null;
  onDone: () => void;
}) {
  return (
    <div
      data-testid="plugin-deeplink-banner"
      className={[
        'mt-6 flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3 font-body text-sm',
        pluginName
          ? 'border-accent-muted bg-accent-muted/40 text-text-primary'
          : 'border-warning-muted bg-warning-muted/30 text-text-secondary',
      ].join(' ')}
    >
      {pluginName ? (
        <p>
          Adding <span className="font-mono font-medium">{pluginName}</span>: click Edit on
          an agent, then Save.
        </p>
      ) : (
        <p>That plugin is not enabled or not available, so nothing is pre-selected.</p>
      )}
      <button
        type="button"
        onClick={onDone}
        className="rounded-md border border-border px-2.5 py-1 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
      >
        Done
      </button>
    </div>
  );
}

function AgentRow({
  agent,
  isCompactor,
  onEdit,
  onToggleEnabled,
  toggling,
}: {
  agent: Agent;
  isCompactor: boolean;
  onEdit: (agent: Agent) => void;
  onToggleEnabled: (agent: Agent) => void;
  toggling: boolean;
}) {
  return (
    <tr
      tabIndex={0}
      aria-label={`Edit ${agent.name}`}
      onClick={() => onEdit(agent)}
      onKeyDown={(e) => {
        if (e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) {
          e.preventDefault();
          onEdit(agent);
        }
      }}
      className="cursor-pointer border-b border-border transition-colors duration-fast last:border-b-0 hover:bg-paper-100 focus-visible:bg-paper-100 focus-visible:outline-none"
    >
      <td className="px-4 py-3">
        <div className="flex flex-wrap items-center gap-2 font-body text-sm font-medium text-text-primary">
          {agent.name}
          {isCompactor && (
            <span className="inline-flex rounded-full bg-accent-muted px-2 py-0.5 font-body text-xs font-medium text-accent">
              Knowledge compactor
            </span>
          )}
        </div>
        {agent.presetSource && (
          <div className="mt-0.5 font-mono text-xs text-text-muted">
            {agent.presetSource}
          </div>
        )}
      </td>
      <td className="px-4 py-3 font-body text-sm text-text-secondary">
        {agent.role}
      </td>
      <td className="px-4 py-3 font-body text-sm text-text-secondary">
        <div>{agent.connector}</div>
        {agent.modelProvider && agent.model && (
          <div className="mt-0.5 font-mono text-xs text-text-muted">
            {agent.modelProvider}/{agent.model}
          </div>
        )}
      </td>
      <td className="px-4 py-3">
        <span
          className={[
            'inline-flex rounded-full px-2 py-0.5 font-body text-xs font-medium',
            agent.enabled
              ? 'bg-moss-100 text-moss-800'
              : 'bg-bark-100 text-bark-500',
          ].join(' ')}
        >
          {agent.enabled ? 'Enabled' : 'Disabled'}
        </span>
      </td>
      <td className="px-4 py-3">
        <HealthBadge health={agent.health} detail={agent.healthDetail} />
      </td>
      <td className="px-4 py-3 font-body text-xs text-text-muted">
        {formatDate(agent.updatedAt)}
      </td>
      <td className="px-4 py-3">
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleEnabled(agent);
            }}
            disabled={toggling}
            className="rounded-md border border-border px-2.5 py-1 font-body text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-50"
          >
            {agent.enabled ? 'Disable' : 'Enable'}
          </button>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onEdit(agent);
            }}
            className="rounded-md border border-border px-2.5 py-1 font-body text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            Edit
          </button>
        </div>
      </td>
    </tr>
  );
}

export function AgentsPage() {
  const { data: agents, isLoading, isError, refetch } = useAgents();
  const { data: presets, isLoading: presetsLoading } = useAgentPresets();
  const { data: connectorOptions = [] } = useConnectors();
  const [createOpen, setCreateOpen] = useState(false);
  const [editingAgent, setEditingAgent] = useState<Agent | null>(null);
  const [editOpen, setEditOpen] = useState(false);
  const [editSession, setEditSession] = useState(0);
  const [togglingId, setTogglingId] = useState<string | null>(null);
  const updateAgentMutation = useUpdateAgentMutation();
  const { data: knowledgeSettings } = useKnowledgeSettings();
  const compactorId = knowledgeSettings?.compactionAgentId ?? null;
  const [searchParams, setSearchParams] = useSearchParams();
  const deepLinkPluginId = searchParams.get('plugin');
  const { data: plugins, isLoading: pluginsLoading } = usePlugins();
  const deepLinkPlugin = deepLinkPluginId
    ? plugins?.find((p) => p.id === deepLinkPluginId && isAssignable(p))
    : undefined;

  function clearDeepLink() {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete('plugin');
        return next;
      },
      { replace: true },
    );
  }

  function openEdit(agent: Agent) {
    setEditingAgent(agent);
    setEditSession((n) => n + 1);
    setEditOpen(true);
  }

  async function toggleEnabled(agent: Agent) {
    setTogglingId(agent.id);
    try {
      await updateAgentMutation.mutateAsync({
        agentId: agent.id,
        body: { enabled: !agent.enabled },
      });
    } catch {
      // list refetches on success
    } finally {
      setTogglingId(null);
    }
  }

  const canCreate = presets && presets.length > 0;

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-display text-2xl font-semibold text-bark-900">
            Agents
          </h1>
          <p className="mt-2 max-w-xl font-body text-text-secondary">
            Configure your agent team from presets.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setCreateOpen(true)}
          disabled={!canCreate || presetsLoading}
          className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
        >
          New agent
        </button>
      </div>

      {deepLinkPluginId && !pluginsLoading && (
        <PluginDeepLinkBanner
          pluginName={deepLinkPlugin?.name ?? null}
          onDone={clearDeepLink}
        />
      )}

      {isLoading && (
        <p className="mt-10 font-body text-sm text-text-muted">
          Loading agents…
        </p>
      )}

      {isError && (
        <div className="mt-10 rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
          <p className="font-body text-sm text-danger">Unable to load agents.</p>
          <button
            type="button"
            onClick={() => void refetch()}
            className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Try again
          </button>
        </div>
      )}

      {!isLoading && !isError && agents && (
        <KnowledgeCompactionCard
          agents={agents}
          canCreateAgent={Boolean(canCreate) && !presetsLoading}
          onCreateAgent={() => setCreateOpen(true)}
        />
      )}

      {!isLoading && !isError && agents?.length === 0 && (
        <div className="mt-10 rounded-xl border border-dashed border-bark-300 bg-paper-100 px-8 py-12 text-center">
          <p className="font-display text-lg font-semibold text-bark-800">
            No agents yet
          </p>
          <p className="mt-2 font-body text-sm text-text-secondary">
            Create an agent from a preset to assign work on tickets.
          </p>
          {canCreate && (
            <button
              type="button"
              onClick={() => setCreateOpen(true)}
              className="mt-6 rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700"
            >
              Create agent
            </button>
          )}
        </div>
      )}

      {!isLoading && !isError && agents && agents.length > 0 && (
        <div className="mt-8 overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-border bg-paper-100">
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Name
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Role
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Connector
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Status
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Health
                </th>
                <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                  Updated
                </th>
                <th className="px-4 py-3" />
              </tr>
            </thead>
            <tbody>
              {agents.map((agent) => (
                <AgentRow
                  key={agent.id}
                  agent={agent}
                  isCompactor={agent.id === compactorId}
                  onEdit={openEdit}
                  onToggleEnabled={(a) => void toggleEnabled(a)}
                  toggling={togglingId === agent.id}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {presets && (
        <CreateAgentDialog
          open={createOpen}
          onClose={() => setCreateOpen(false)}
          presets={presets}
          connectorOptions={connectorOptions}
        />
      )}

      {editingAgent && (
        <EditAgentDialog
          key={`${editingAgent.id}:${editSession}`}
          open={editOpen}
          agent={editingAgent}
          onClose={() => setEditOpen(false)}
          connectorOptions={connectorOptions}
          preselectPluginId={deepLinkPlugin?.id}
        />
      )}
    </div>
  );
}
