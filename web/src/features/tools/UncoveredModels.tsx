import { useState } from 'react';
import { parseApiErrorMessage } from '../../lib/api';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import { useConnectorModels, type ModelOption } from '../agents/useAgents';
import { MODELS_NOT_COVERED_HEADING, modelsListFailed } from '../runs/reportedModel';
import {
  useReplaceUncoveredModels,
  useUncoveredModels,
} from './useUncoveredModels';

function liveModels(groups: { models: ModelOption[] }[]): ModelOption[] {
  const seen = new Set<string>();
  const models: ModelOption[] = [];
  for (const group of groups) {
    for (const model of group.models) {
      if (seen.has(model.id)) continue;
      seen.add(model.id);
      models.push(model);
    }
  }
  return models;
}

export function UncoveredModels({ connector }: { connector: ConnectorStatus }) {
  const catalog = useConnectorModels(connector.id);
  const saved = useUncoveredModels(connector.id);
  const replace = useReplaceUncoveredModels(connector.id);
  const [error, setError] = useState<string | null>(null);
  const savedIds = saved.data ?? [];
  const listed = liveModels(catalog.groups);
  const listedIds = new Set(listed.map((model) => model.id));
  const rows: ModelOption[] = [
    ...listed,
    ...savedIds
      .filter((id) => !listedIds.has(id))
      .map((id) => ({ id, name: id })),
  ];
  const cannotEdit = saved.isLoading || saved.isError || replace.isPending;

  async function toggle(modelId: string, checked: boolean) {
    const next = checked
      ? [...savedIds.filter((id) => id !== modelId), modelId]
      : savedIds.filter((id) => id !== modelId);
    setError(null);
    try {
      await replace.mutateAsync(next);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Could not update this connector.'));
    }
  }

  return (
    <section className="mt-4" aria-label={MODELS_NOT_COVERED_HEADING}>
      <h4 className="font-body text-xs font-medium text-text-secondary">
        {MODELS_NOT_COVERED_HEADING}
      </h4>
      {catalog.isError && connector.cli.found && (
        <p className="mt-1 font-body text-xs text-text-muted">
          {modelsListFailed(connector.displayName)}
        </p>
      )}
      {saved.isError && (
        <p className="mt-1 font-body text-xs text-danger">
          Could not update this connector.
        </p>
      )}
      {rows.length > 0 && (
        <ul className="mt-2 max-h-48 space-y-1.5 overflow-y-auto">
          {rows.map((model) => (
            <li key={model.id}>
              <label className="flex items-start gap-2">
                <input
                  type="checkbox"
                  checked={savedIds.includes(model.id)}
                  disabled={cannotEdit}
                  onChange={(event) => void toggle(model.id, event.target.checked)}
                  className="mt-0.5 h-4 w-4 rounded border-border text-moss-600 focus:ring-moss-500"
                />
                <span className="font-body text-sm text-text-primary">
                  {model.name}
                  {model.name !== model.id && (
                    <span className="ml-2 font-mono text-xs text-text-muted">
                      {model.id}
                    </span>
                  )}
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}
      {error && (
        <p role="alert" className="mt-1 font-body text-xs text-danger">
          {error}
        </p>
      )}
    </section>
  );
}
