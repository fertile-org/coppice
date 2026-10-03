import { Sparkles, Wrench } from 'lucide-react';
import type {
  RunToolCall,
  ToolCallSource,
  ToolCallStatus,
} from '../../lib/schemas/toolCall';
import { useRunToolCalls } from './useRunToolCalls';

interface RunToolsAndSkillsProps {
  runId: string;
  enabled: boolean;
}

const SOURCE_LABELS: Record<ToolCallSource, string> = {
  core: 'Core',
  skill: 'Skill',
  plugin: 'Plugin',
};

function statusPillClass(status: ToolCallStatus): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  switch (status) {
    case 'ok':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'error':
    case 'timeout':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
    case 'denied':
      return `${base} border-border bg-paper-200 text-text-secondary`;
  }
}

function pluginLabel(call: RunToolCall): string {
  return call.pluginName ?? call.pluginId ?? 'plugin';
}

function ToolCallRow({ call }: { call: RunToolCall }) {
  return (
    <li
      aria-label={`Tool call ${call.tool}`}
      className="rounded-md border border-border bg-paper-50 px-3 py-2"
    >
      <div className="flex flex-wrap items-center gap-2">
        <code className="font-mono text-xs font-medium text-bark-900">
          {call.tool}
        </code>
        <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
          {SOURCE_LABELS[call.source]}
        </span>
        {call.source === 'plugin' && (
          <span className="font-body text-xs text-text-secondary">
            {pluginLabel(call)}
          </span>
        )}
        <span className={statusPillClass(call.status)}>{call.status}</span>
        <span className="ml-auto font-body text-xs text-text-muted">
          {call.durationMs} ms
        </span>
      </div>
      {call.error && call.status !== 'ok' && (
        <p className="mt-1 font-body text-xs text-danger break-words">
          {call.error}
        </p>
      )}
      {call.argsSummary && call.argsSummary !== '{}' && (
        <p
          className="mt-1 truncate font-mono text-[11px] text-text-muted"
          title={call.argsSummary}
        >
          {call.argsSummary}
        </p>
      )}
    </li>
  );
}

export function RunToolsAndSkills({ runId, enabled }: RunToolsAndSkillsProps) {
  const { data, isLoading, isError } = useRunToolCalls(runId, enabled);

  if (!enabled) return null;

  return (
    <section aria-label="Tools & Skills" className="space-y-3">
      {isLoading && (
        <p className="font-body text-xs text-text-muted">Loading tool calls…</p>
      )}
      {isError && (
        <p className="font-body text-xs text-danger">
          Unable to load tool calls.
        </p>
      )}
      {!isLoading && !isError && data?.items.length === 0 && (
        <p className="font-body text-xs text-text-muted">
          No tool calls recorded.
        </p>
      )}

      {data && data.skillsUsed.length > 0 && (
        <div>
          <div className="flex items-center gap-2">
            <Sparkles className="size-4 text-moss-600" aria-hidden="true" />
            <h4 className="font-display text-sm font-semibold text-bark-800">
              Skills used
            </h4>
          </div>
          <ul aria-label="Skills used" className="mt-2 flex flex-wrap gap-1.5">
            {data.skillsUsed.map((skill) => (
              <li
                key={skill}
                className="rounded-full border border-moss-200 bg-moss-50 px-2 py-0.5 font-mono text-xs text-moss-800"
              >
                {skill}
              </li>
            ))}
          </ul>
        </div>
      )}

      {data && data.items.length > 0 && (
        <div>
          <div className="flex items-center gap-2">
            <Wrench className="size-4 text-moss-600" aria-hidden="true" />
            <h4 className="font-display text-sm font-semibold text-bark-800">
              Tool calls
            </h4>
            <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs text-moss-800">
              {data.items.length}
            </span>
          </div>
          <ol className="mt-2 space-y-1.5">
            {data.items.map((call) => (
              <ToolCallRow key={call.id} call={call} />
            ))}
          </ol>
        </div>
      )}
    </section>
  );
}
