import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../lib/api';
import {
  AgentForm,
  agentToFormValues,
  listFromLines,
  presetToFormValues,
  type AgentFormValues,
} from './AgentForm';
import {
  useAgentPresets,
  useAgents,
  useCreateAgent,
  useUpdateAgent,
  useUpdateAgentMutation,
  type Agent,
  type AgentPreset,
} from './useAgents';

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

function CreateAgentDialog({
  open,
  onClose,
  presets,
}: {
  open: boolean;
  onClose: () => void;
  presets: AgentPreset[];
}) {
  const presetRef = useRef<HTMLSelectElement>(null);
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
  const createAgent = useCreateAgent();

  useEffect(() => {
    if (!open) return;
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

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  function handlePresetChange(nextPresetId: string) {
    setPresetId(nextPresetId);
    const preset = presets.find((p) => p.id === nextPresetId);
    if (preset) {
      setValues((prev) => presetToFormValues(preset, prev.name));
    }
  }

  async function handleSubmit(formValues: AgentFormValues) {
    setError(null);
    try {
      await createAgent.mutateAsync({
        name: formValues.name.trim(),
        presetId: presetId || undefined,
      });
      onClose();
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) {
        setError('Invalid agent configuration.');
      } else {
        setError('Unable to create agent. Please try again.');
      }
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-bark-950/40 px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-agent-title"
        className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-xl border border-border bg-paper-50 p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="create-agent-title"
          className="font-display text-xl font-semibold text-bark-900"
        >
          New agent
        </h2>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Choose a preset to prefill role and prompt, then name your agent.
        </p>

        <div className="mt-5">
          <label
            htmlFor="agent-preset"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Preset
          </label>
          <select
            ref={presetRef}
            id="agent-preset"
            value={presetId}
            onChange={(e) => handlePresetChange(e.target.value)}
            className="w-full rounded-md border border-border bg-surface-raised px-3 py-2 font-body text-sm text-text-primary outline-none transition-colors duration-fast focus:border-moss-500 focus:ring-2 focus:ring-moss-100"
          >
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.key} — {preset.role}
              </option>
            ))}
          </select>
        </div>

        <div className="mt-4">
          <AgentForm
            mode="create"
            values={values}
            onChange={setValues}
            onSubmit={handleSubmit}
            onCancel={onClose}
            isPending={createAgent.isPending}
            error={error}
          />
        </div>
      </div>
    </div>
  );
}

function EditAgentDialog({
  agent,
  onClose,
}: {
  agent: Agent;
  onClose: () => void;
}) {
  const [values, setValues] = useState<AgentFormValues>(() =>
    agentToFormValues(agent),
  );
  const [error, setError] = useState<string | null>(null);
  const updateAgent = useUpdateAgent(agent.id);

  useEffect(() => {
    setValues(agentToFormValues(agent));
    setError(null);
  }, [agent]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  async function handleSubmit(formValues: AgentFormValues) {
    setError(null);
    try {
      await updateAgent.mutateAsync({
        name: formValues.name.trim(),
        role: formValues.role.trim(),
        skills: listFromLines(formValues.skills),
        responsibilities: listFromLines(formValues.responsibilities),
        systemPrompt: formValues.systemPrompt,
        enabled: formValues.enabled,
      });
      onClose();
    } catch {
      setError('Unable to save agent.');
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-bark-950/40 px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="edit-agent-title"
        className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-xl border border-border bg-paper-50 p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="edit-agent-title"
          className="font-display text-xl font-semibold text-bark-900"
        >
          Edit agent
        </h2>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Update {agent.name}&apos;s configuration.
        </p>

        <div className="mt-5">
          <AgentForm
            mode="edit"
            values={values}
            onChange={setValues}
            onSubmit={handleSubmit}
            onCancel={onClose}
            isPending={updateAgent.isPending}
            error={error}
          />
        </div>
      </div>
    </div>
  );
}

function AgentRow({
  agent,
  onEdit,
  onToggleEnabled,
  toggling,
}: {
  agent: Agent;
  onEdit: (agent: Agent) => void;
  onToggleEnabled: (agent: Agent) => void;
  toggling: boolean;
}) {
  return (
    <tr className="border-b border-border last:border-b-0">
      <td className="px-4 py-3">
        <div className="font-body text-sm font-medium text-text-primary">
          {agent.name}
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
      <td className="px-4 py-3 font-body text-xs text-text-muted">
        {formatDate(agent.updatedAt)}
      </td>
      <td className="px-4 py-3">
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => onToggleEnabled(agent)}
            disabled={toggling}
            className="rounded-md border border-border px-2.5 py-1 font-body text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-50"
          >
            {agent.enabled ? 'Disable' : 'Enable'}
          </button>
          <button
            type="button"
            onClick={() => onEdit(agent)}
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
  const [createOpen, setCreateOpen] = useState(false);
  const [editingAgent, setEditingAgent] = useState<Agent | null>(null);
  const [togglingId, setTogglingId] = useState<string | null>(null);
  const updateAgentMutation = useUpdateAgentMutation();

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
                  Status
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
                  onEdit={setEditingAgent}
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
        />
      )}

      {editingAgent && (
        <EditAgentDialog
          agent={editingAgent}
          onClose={() => setEditingAgent(null)}
        />
      )}
    </div>
  );
}
