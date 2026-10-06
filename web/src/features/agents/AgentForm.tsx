import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Combobox } from '../../components/ui/combobox';
import {
  INSTALL_GUIDE_URL,
  NO_READY_CONNECTOR_HINT,
  notOnPathHint,
  notSignedInHint,
  readinessLabel,
} from '../tools/connectorCopy';
import type {
  Agent,
  AgentPreset,
  ConnectorOption,
} from './useAgents';
import { useModelProviders, useModels } from './useAgents';
import { isAssignable } from '../plugins/assignable';
import { usePlugins } from '../plugins/usePlugins';

export interface AgentFormValues {
  name: string;
  role: string;
  skills: string;
  responsibilities: string;
  systemPrompt: string;
  connector: string;
  modelProvider: string;
  model: string;
  enabled: boolean;
  pluginIds: string[];
}

/** Whether the agent's current plugin assignment is known, so picks can be edited. */
export type PluginAssignmentState =
  | { status: 'ready' }
  | { status: 'loading' }
  | { status: 'error'; message: string };

function linesFromList(items: string[]): string {
  return items.join('\n');
}

function listFromLines(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

export function agentToFormValues(agent: Agent): AgentFormValues {
  return {
    name: agent.name,
    role: agent.role,
    skills: linesFromList(agent.skills),
    responsibilities: linesFromList(agent.responsibilities),
    systemPrompt: agent.systemPrompt,
    connector: agent.connector,
    modelProvider: agent.modelProvider ?? '',
    model: agent.model ?? '',
    enabled: agent.enabled,
    pluginIds: [],
  };
}

export function presetToFormValues(
  preset: AgentPreset,
  name = '',
  connector = '',
): AgentFormValues {
  return {
    name,
    role: preset.role,
    skills: linesFromList(preset.skills),
    responsibilities: linesFromList(preset.responsibilities),
    systemPrompt: preset.systemPromptTemplate,
    connector,
    modelProvider: '',
    model: '',
    enabled: true,
    pluginIds: [],
  };
}

function connectorOptionLabel(option: ConnectorOption): string {
  const name = option.displayName ?? option.id;
  const status = readinessLabel(option.readiness);
  return status ? `${name} — ${status}` : name;
}

function ConnectorHints({
  values,
  connectorOptions,
}: {
  values: AgentFormValues;
  connectorOptions: ConnectorOption[];
}) {
  const selected = connectorOptions.find((option) => option.id === values.connector);
  const name = selected?.displayName ?? selected?.id ?? '';
  const stillChecking = connectorOptions.some(
    (option) => option.id !== 'mock' && option.readiness == null,
  );
  const noneReady =
    connectorOptions.length > 0 &&
    !stillChecking &&
    !connectorOptions.some((option) => option.readiness === 'ready');

  return (
    <>
      {noneReady && (
        <p className="mt-2 font-body text-xs text-text-secondary">{NO_READY_CONNECTOR_HINT}</p>
      )}
      {selected?.readiness === 'not_on_path' && (
        <p className="mt-2 font-body text-xs text-text-secondary">
          {notOnPathHint(name)}{' '}
          <a
            href={INSTALL_GUIDE_URL}
            target="_blank"
            rel="noreferrer"
            className="font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Install guide
          </a>
        </p>
      )}
      {selected?.readiness === 'found_not_signed_in' && (
        <p className="mt-2 font-body text-xs text-text-secondary">{notSignedInHint(name)}</p>
      )}
    </>
  );
}

interface AgentFormProps {
  mode: 'create' | 'edit';
  values: AgentFormValues;
  onChange: (values: AgentFormValues) => void;
  onSubmit: (values: AgentFormValues) => void | Promise<void>;
  onCancel: () => void;
  connectorOptions: ConnectorOption[];
  isPending?: boolean;
  error?: string | null;
  submitLabel?: string;
  pluginAssignment?: PluginAssignmentState;
  /** Scrolls to and highlights the plugin picker (plugin deep link). */
  focusPlugins?: boolean;
}

export function AgentForm({
  mode,
  values,
  onChange,
  onSubmit,
  onCancel,
  connectorOptions,
  isPending = false,
  error = null,
  submitLabel,
  pluginAssignment = { status: 'ready' },
  focusPlugins = false,
}: AgentFormProps) {
  const [localError, setLocalError] = useState<string | null>(null);
  const pluginsRef = useRef<HTMLFieldSetElement>(null);
  const showModelFields = values.connector !== 'mock';
  const { data: modelProviderOptions = [], isLoading: modelProvidersLoading } =
    useModelProviders(showModelFields ? values.connector : undefined);
  const { data: modelOptions = [], isLoading: modelsLoading } = useModels(
    showModelFields ? values.connector : undefined,
    showModelFields ? values.modelProvider : undefined,
  );
  const { data: plugins, isLoading: pluginsLoading } = usePlugins();
  const assignablePlugins = (plugins ?? []).filter(isAssignable);
  const assignableIds = new Set(assignablePlugins.map((p) => p.id));
  const unavailableAssigned = plugins
    ? values.pluginIds
        .filter((id) => !assignableIds.has(id))
        .map((id) => ({ id, name: plugins.find((p) => p.id === id)?.name ?? id }))
    : [];

  useEffect(() => {
    setLocalError(null);
  }, [values]);

  useEffect(() => {
    if (focusPlugins) pluginsRef.current?.scrollIntoView?.({ block: 'center' });
  }, [focusPlugins]);

  function updateField<K extends keyof AgentFormValues>(
    key: K,
    value: AgentFormValues[K],
  ) {
    onChange({ ...values, [key]: value });
  }

  function handleConnectorChange(connector: string) {
    onChange({ ...values, connector, modelProvider: '', model: '' });
  }

  function handleModelProviderChange(modelProvider: string) {
    onChange({ ...values, modelProvider, model: '' });
  }

  function togglePlugin(id: string, checked: boolean) {
    const pluginIds = checked
      ? [...values.pluginIds, id]
      : values.pluginIds.filter((existing) => existing !== id);
    onChange({ ...values, pluginIds });
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!values.name.trim()) {
      setLocalError('Name is required.');
      return;
    }
    if (mode === 'edit' && !values.role.trim()) {
      setLocalError('Role is required.');
      return;
    }
    if (!values.connector) {
      setLocalError('Choose a connector.');
      return;
    }
    setLocalError(null);
    const pluginIds = plugins
      ? values.pluginIds.filter((id) => assignableIds.has(id))
      : values.pluginIds;
    await onSubmit({ ...values, pluginIds });
  }

  const displayError = error ?? localError;

  return (
    <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
      <div>
        <label
          htmlFor="agent-name"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Name
        </label>
        <input
          id="agent-name"
          type="text"
          required
          value={values.name}
          onChange={(e) => updateField('name', e.target.value)}
          placeholder="e.g. PM Bot"
          className="field-control w-full px-3 py-2 font-body text-sm"
        />
      </div>

      <div>
        <label
          htmlFor="agent-role"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Role
        </label>
        <input
          id="agent-role"
          type="text"
          value={values.role}
          onChange={(e) => updateField('role', e.target.value)}
          readOnly={mode === 'create'}
          className="field-control w-full px-3 py-2 font-body text-sm"
        />
      </div>

      <div>
        <label
          htmlFor="agent-skills"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Skills
          <span className="ml-1 font-normal text-text-muted">(one per line)</span>
        </label>
        <textarea
          id="agent-skills"
          rows={3}
          value={values.skills}
          onChange={(e) => updateField('skills', e.target.value)}
          readOnly={mode === 'create'}
          className="field-control w-full resize-y px-3 py-2 font-body text-sm"
        />
      </div>

      <fieldset
        ref={pluginsRef}
        disabled={pluginAssignment.status !== 'ready'}
        className={
          focusPlugins ? 'rounded-md p-2 ring-2 ring-accent ring-offset-2' : undefined
        }
      >
        <legend className="mb-1 block font-body text-sm font-medium text-bark-800">
          Plugins
          <span className="ml-1 font-normal text-text-muted">
            (skills exposed as plugin:skill)
          </span>
        </legend>
        {pluginAssignment.status === 'loading' && (
          <p className="mb-1.5 font-body text-sm text-text-muted">
            Loading assigned plugins…
          </p>
        )}
        {pluginAssignment.status === 'error' && (
          <p className="mb-1.5 font-body text-sm text-danger">
            Plugin changes are unavailable. Could not load this agent&apos;s
            plugins: {pluginAssignment.message}
          </p>
        )}
        {pluginsLoading ? (
          <p className="font-body text-sm text-text-muted">Loading plugins…</p>
        ) : assignablePlugins.length === 0 && unavailableAssigned.length === 0 ? (
          <p className="font-body text-sm text-text-muted">
            No enabled plugins. Enable plugins on the{' '}
            <Link
              to="/settings/plugins"
              className="font-medium text-moss-700 underline-offset-2 hover:underline"
            >
              Plugins
            </Link>{' '}
            page.
          </p>
        ) : (
          <ul className="space-y-1.5">
            {assignablePlugins.map((plugin) => {
              const skillCount = plugin.skills.filter((skill) => skill.enabled).length;
              return (
                <li key={plugin.id}>
                  <label className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      checked={values.pluginIds.includes(plugin.id)}
                      onChange={(e) => togglePlugin(plugin.id, e.target.checked)}
                      className="mt-0.5 h-4 w-4 rounded border-border text-moss-600 focus:ring-moss-500"
                    />
                    <span className="font-body text-sm text-text-primary">
                      {plugin.name}
                      {skillCount > 0 && (
                        <span className="ml-1 text-text-muted">
                          ({skillCount} {skillCount === 1 ? 'skill' : 'skills'})
                        </span>
                      )}
                    </span>
                  </label>
                </li>
              );
            })}
            {unavailableAssigned.map((plugin) => (
              <li key={plugin.id}>
                <label className="flex items-start gap-2 opacity-70">
                  <input
                    type="checkbox"
                    checked
                    disabled
                    className="mt-0.5 h-4 w-4 rounded border-border"
                  />
                  <span className="font-body text-sm text-text-secondary">
                    {plugin.name}
                    <span className="ml-1 text-amber-900">
                      — unavailable; will be removed on save
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}
      </fieldset>

      <div>
        <label
          htmlFor="agent-responsibilities"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Responsibilities
          <span className="ml-1 font-normal text-text-muted">(one per line)</span>
        </label>
        <textarea
          id="agent-responsibilities"
          rows={3}
          value={values.responsibilities}
          onChange={(e) => updateField('responsibilities', e.target.value)}
          readOnly={mode === 'create'}
          className="field-control w-full resize-y px-3 py-2 font-body text-sm"
        />
      </div>

      <div>
        <label
          htmlFor="agent-system-prompt"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          System prompt
        </label>
        <textarea
          id="agent-system-prompt"
          rows={20}
          value={values.systemPrompt}
          onChange={(e) => updateField('systemPrompt', e.target.value)}
          className="field-control w-full resize-y px-3 py-2 font-mono text-sm leading-relaxed"
        />
      </div>

      <div>
        <label
          htmlFor="agent-connector"
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Connector
        </label>
        <Combobox
          id="agent-connector"
          value={values.connector}
          onValueChange={handleConnectorChange}
          searchPlaceholder="Search connectors…"
          options={connectorOptions.map((option) => ({
            value: option.id,
            label: connectorOptionLabel(option),
          }))}
        />
        <ConnectorHints values={values} connectorOptions={connectorOptions} />
      </div>

      {showModelFields && (
        <>
          <div>
            <label
              htmlFor="agent-model-provider"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Model provider
            </label>
            <Combobox
              id="agent-model-provider"
              value={values.modelProvider}
              onValueChange={handleModelProviderChange}
              disabled={modelProvidersLoading}
              placeholder={modelProvidersLoading ? 'Loading…' : 'Select model provider'}
              searchPlaceholder="Search providers…"
              clearable
              options={modelProviderOptions.map((p) => ({ value: p.id, label: p.id }))}
            />
          </div>

          <div>
            <label
              htmlFor="agent-model"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Model
            </label>
            <Combobox
              id="agent-model"
              value={values.model}
              onValueChange={(value) => updateField('model', value)}
              disabled={!values.modelProvider || modelsLoading}
              placeholder={modelsLoading ? 'Loading…' : 'Select model'}
              searchPlaceholder="Search models…"
              clearable
              options={modelOptions.map((m) => ({ value: m.id, label: m.name }))}
            />
          </div>
        </>
      )}

      {mode === 'edit' && (
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={values.enabled}
            onChange={(e) => updateField('enabled', e.target.checked)}
            className="h-4 w-4 rounded border-border text-moss-600 focus:ring-moss-500"
          />
          <span className="font-body text-sm text-text-primary">Enabled</span>
        </label>
      )}

      {displayError && (
        <p
          role="alert"
          className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {displayError}
        </p>
      )}

      <div className="flex justify-end gap-2 pt-1">
        <button
          type="button"
          onClick={onCancel}
          disabled={isPending}
          className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-bark-300 hover:text-text-primary disabled:opacity-60"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={isPending}
          className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
        >
          {isPending
            ? 'Saving…'
            : submitLabel ?? (mode === 'create' ? 'Create agent' : 'Save changes')}
        </button>
      </div>
    </form>
  );
}

export { listFromLines };
