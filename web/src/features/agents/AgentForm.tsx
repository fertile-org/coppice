import { useEffect, useState, type FormEvent } from 'react';
import type { Agent, AgentPreset } from './useAgents';

export interface AgentFormValues {
  name: string;
  role: string;
  skills: string;
  responsibilities: string;
  systemPrompt: string;
  providerId: string;
  enabled: boolean;
}

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
    providerId: agent.providerId,
    enabled: agent.enabled,
  };
}

export function presetToFormValues(
  preset: AgentPreset,
  name = '',
): AgentFormValues {
  return {
    name,
    role: preset.role,
    skills: linesFromList(preset.skills),
    responsibilities: linesFromList(preset.responsibilities),
    systemPrompt: preset.systemPromptTemplate,
    providerId: 'mock',
    enabled: true,
  };
}

interface AgentFormProps {
  mode: 'create' | 'edit';
  values: AgentFormValues;
  onChange: (values: AgentFormValues) => void;
  onSubmit: (values: AgentFormValues) => void | Promise<void>;
  onCancel: () => void;
  isPending?: boolean;
  error?: string | null;
  submitLabel?: string;
}

export function AgentForm({
  mode,
  values,
  onChange,
  onSubmit,
  onCancel,
  isPending = false,
  error = null,
  submitLabel,
}: AgentFormProps) {
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    setLocalError(null);
  }, [values]);

  function updateField<K extends keyof AgentFormValues>(
    key: K,
    value: AgentFormValues[K],
  ) {
    onChange({ ...values, [key]: value });
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
    setLocalError(null);
    await onSubmit(values);
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
          rows={6}
          value={values.systemPrompt}
          onChange={(e) => updateField('systemPrompt', e.target.value)}
          readOnly={mode === 'create'}
          className="field-control w-full resize-y px-3 py-2 font-mono text-sm leading-relaxed"
        />
      </div>

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
