import { useEffect, useRef, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { ApiError } from '../../lib/api';
import { useSession } from '../auth/useSession';
import { formatInterval, pluralTickets } from '../knowledge/compactionFormat';
import { useCompactionStatus } from '../knowledge/useCompaction';
import {
  useKnowledgeSettings,
  useUpdateKnowledgeSettings,
} from '../settings/useKnowledgeSettings';
import type { Agent } from './useAgents';

export const KNOWLEDGE_COMPACTION_ANCHOR = 'knowledge-compaction';
const NO_READ_ONLY_REASON = 'Cannot run compaction (no read-only mode)';

function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    try {
      const body = JSON.parse(error.body) as { message?: string };
      if (body.message) return body.message;
    } catch {
      // fall through
    }
    if (error.status === 403) return 'Only admins can change the compaction agent.';
  }
  return 'Unable to save the compaction agent.';
}

export function KnowledgeCompactionCard({
  agents,
  canCreateAgent,
  onCreateAgent,
}: {
  agents: Agent[];
  canCreateAgent: boolean;
  onCreateAgent: () => void;
}) {
  const { user } = useSession();
  const isAdmin = user?.role === 'admin';
  const { hash } = useLocation();
  const settingsQuery = useKnowledgeSettings();
  const statusQuery = useCompactionStatus();
  const update = useUpdateKnowledgeSettings();
  const sectionRef = useRef<HTMLElement>(null);
  const selectRef = useRef<HTMLSelectElement>(null);
  const savedId = settingsQuery.data?.compactionAgentId ?? '';
  const [draft, setDraft] = useState<string | null>(null);
  const selected = draft ?? savedId;
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (hash !== `#${KNOWLEDGE_COMPACTION_ANCHOR}` || !settingsQuery.data) return;
    sectionRef.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' });
    selectRef.current?.focus();
  }, [hash, settingsQuery.data]);

  const readOnlyConnectors = settingsQuery.data?.readOnlyConnectors ?? [];
  const current = settingsQuery.data?.compactionAgent ?? null;
  const canRunCompaction = (agent: Pick<Agent, 'connector'>) =>
    readOnlyConnectors.includes(agent.connector);
  const options = agents.filter(
    (agent) => agent.enabled || agent.id === savedId,
  );
  const selectedAgent = agents.find((agent) => agent.id === selected) ?? null;
  const selectedInvalid = selectedAgent !== null && !canRunCompaction(selectedAgent);
  const dirty = selected !== savedId;
  const status = statusQuery.data;

  async function save() {
    setError(null);
    try {
      await update.mutateAsync(selected || null);
      setDraft(null);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section
      ref={sectionRef}
      id={KNOWLEDGE_COMPACTION_ANCHOR}
      aria-labelledby="knowledge-compaction-title"
      className="mt-8 scroll-mt-6 rounded-xl border border-border bg-surface-raised p-5 shadow-card"
    >
      <h2
        id="knowledge-compaction-title"
        className="font-display text-lg font-semibold text-bark-900"
      >
        Knowledge compaction
      </h2>
      <p className="mt-1 max-w-2xl font-body text-sm text-text-secondary">
        Agent that turns Done tickets into knowledge candidates for review.
        Leave empty to turn compaction off.
      </p>

      {settingsQuery.isLoading && (
        <p className="mt-4 font-body text-sm text-text-muted">Loading…</p>
      )}
      {settingsQuery.isError && (
        <p role="alert" className="mt-4 font-body text-sm text-danger">
          Unable to load the compaction setting.
        </p>
      )}

      {settingsQuery.data && (
        <div className="mt-4 flex flex-wrap items-end gap-3">
          <div className="min-w-[16rem] flex-1 sm:max-w-md">
            <label
              htmlFor="compaction-agent"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Compaction agent
            </label>
            {isAdmin ? (
              <select
                ref={selectRef}
                id="compaction-agent"
                value={selected}
                disabled={agents.length === 0 || update.isPending}
                onChange={(event) => {
                  setDraft(event.target.value);
                  setError(null);
                }}
                className="field-control w-full px-3 py-2 font-body text-sm"
              >
                <option value="">None — compaction off</option>
                {options.map((agent) => {
                  const capable = canRunCompaction(agent);
                  const suffix = [
                    !agent.enabled ? '(disabled)' : null,
                    !capable ? `— ${NO_READ_ONLY_REASON}` : null,
                  ]
                    .filter(Boolean)
                    .join(' ');
                  return (
                    <option
                      key={agent.id}
                      value={agent.id}
                      disabled={!capable && agent.id !== savedId}
                    >
                      {agent.name} ({agent.connector}){suffix ? ` ${suffix}` : ''}
                    </option>
                  );
                })}
              </select>
            ) : (
              <p
                id="compaction-agent"
                className="rounded-md border border-border bg-paper-100 px-3 py-2 font-body text-sm text-text-primary"
              >
                {current
                  ? `${current.name} (${current.connector})${current.enabled ? '' : ' (disabled)'}`
                  : 'None — compaction off'}
              </p>
            )}
          </div>
          {isAdmin && (
            <button
              type="button"
              onClick={() => void save()}
              disabled={!dirty || selectedInvalid || update.isPending}
              className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
            >
              {update.isPending ? 'Saving…' : 'Save'}
            </button>
          )}
        </div>
      )}

      {settingsQuery.data && agents.length === 0 && (
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <p className="font-body text-sm text-text-secondary">Create an agent first.</p>
          {canCreateAgent && (
            <button
              type="button"
              onClick={onCreateAgent}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm font-medium text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              New agent
            </button>
          )}
        </div>
      )}

      {current && !current.enabled && !dirty && (
        <p className="mt-3 rounded-md bg-amber-100 px-3 py-2 font-body text-sm text-amber-900">
          This agent is disabled; compaction is paused.
        </p>
      )}
      {selectedInvalid && (
        <p className="mt-3 rounded-md bg-amber-100 px-3 py-2 font-body text-sm text-amber-900">
          {NO_READ_ONLY_REASON}. Compaction needs a connector that enforces
          read-only tools: {readOnlyConnectors.join(', ')}.
        </p>
      )}
      {error && (
        <p role="alert" className="mt-3 font-body text-sm text-danger">
          {error}
        </p>
      )}

      {status && (
        <p className="mt-4 font-body text-xs text-text-muted">
          Runs every {formatInterval(status.intervalSecs)} in batches of up to{' '}
          {pluralTickets(status.batchMaxTickets)}, read-only, no repo.{' '}
          {pluralTickets(status.queuedCount, 'Done ticket')}{' '}
          {status.queuedCount === 1 ? 'is' : 'are'} waiting.
        </p>
      )}
    </section>
  );
}
