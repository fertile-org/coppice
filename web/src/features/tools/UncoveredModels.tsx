import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { parseApiErrorMessage } from '../../lib/api';
import type { ConnectorStatus } from '../../lib/schemas/connectorDiagnostics';
import { RefreshModelsButton } from '../agents/RefreshModelsButton';
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
  const [expanded, setExpanded] = useState(false);
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
  const visible = expanded ? rows : rows.filter((model) => savedIds.includes(model.id));
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
    <section className="mt-4">
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
        {MODELS_NOT_COVERED_HEADING}
      </button>
      {catalog.isError && connector.cli.found && (
        <p className="mt-1 flex flex-wrap items-baseline gap-x-2 font-body text-xs text-text-muted">
          <span>{modelsListFailed(connector.displayName)}</span>
          <RefreshModelsButton
            onClick={() => catalog.refresh()}
            disabled={catalog.isLoading || catalog.isFetching}
          />
        </p>
      )}
      {saved.isError && (
        <p className="mt-1 font-body text-xs text-danger">
          Could not update this connector.
        </p>
      )}
      {visible.length > 0 && (
        <ul className="mt-2 max-h-48 space-y-1.5 overflow-y-auto">
          {visible.map((model) => (
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
