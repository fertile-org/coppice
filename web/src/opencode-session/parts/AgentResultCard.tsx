import { MarkdownContent } from '../components/MarkdownContent';
import { sessionTheme } from '../theme/session-theme';
import type { AgentResultContract } from './parse-result-contract';

function MetaList({ label, items }: { label: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div className="mt-3 border-t border-[var(--oc-border)] pt-3">
      <p className={`mb-1 ${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}>
        {label}
      </p>
      <ul className={`list-disc space-y-0.5 pl-4 ${sessionTheme.fontMonoSm} ${sessionTheme.text}`}>
        {items.map((item) => (
          <li key={item} className="break-all">
            {item}
          </li>
        ))}
      </ul>
    </div>
  );
}

function shouldShowNextStatus(contract: AgentResultContract): boolean {
  if (!contract.nextStatus) return false;
  const next = contract.nextStatus.trim().toLowerCase();
  if (contract.status === 'done' && next === 'done') return false;
  if (contract.status === 'blocked' && next === 'blocked') return false;
  return true;
}

export function AgentResultCard({ contract }: { contract: AgentResultContract }) {
  const isDone = contract.status === 'done';
  const showNextStatus = shouldShowNextStatus(contract);

  return (
    <div className={`overflow-hidden border border-[var(--oc-border)] ${sessionTheme.bgElement}`}>
      <div
        className={`flex flex-wrap items-center gap-2 border-b border-[var(--oc-border)] px-3 py-2`}
      >
        <span
          className={`px-2 py-0.5 ${sessionTheme.fontMonoSm} font-medium ${
            isDone ? sessionTheme.success : sessionTheme.warning
          }`}
        >
          {isDone ? 'Done' : 'Blocked'}
        </span>
        {!isDone && contract.blockerType ? (
          <span className={`${sessionTheme.fontMonoSm} ${sessionTheme.secondary}`}>
            {contract.blockerType.replaceAll('_', ' ')}
          </span>
        ) : null}
        {showNextStatus ? (
          <span className={`${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}>
            → {contract.nextStatus}
          </span>
        ) : null}
      </div>

      <div className="px-3 py-3">
        <MarkdownContent>{contract.summary}</MarkdownContent>

        {contract.acceptanceCriteria?.trim() ? (
          <div className="mt-3 border-t border-[var(--oc-border)] pt-3">
            <MarkdownContent>{contract.acceptanceCriteria}</MarkdownContent>
          </div>
        ) : null}

        {isDone ? (
          <>
            <MetaList label="Changed files" items={contract.changedFiles ?? []} />
            <MetaList label="Tests run" items={contract.testsRun ?? []} />
            <MetaList label="Blockers" items={contract.blockers ?? []} />
          </>
        ) : (
          <>
            <MetaList
              label="Required capabilities"
              items={contract.requiredCapabilities ?? []}
            />
            <MetaList label="Required secrets" items={contract.requiredSecrets ?? []} />
          </>
        )}

        {contract.status === 'done' && contract.assignTo ? (
          <p className={`mt-3 ${sessionTheme.fontMonoSm} ${sessionTheme.textMuted}`}>
            Assign to:{' '}
            <span className={sessionTheme.text}>{contract.assignTo}</span>
          </p>
        ) : null}

        {(contract.mentionAgents?.length ?? 0) > 0 ? (
          <div className="mt-3 flex flex-wrap gap-1">
            {contract.mentionAgents!.map((agent) => (
              <span
                key={agent}
                className={`px-2 py-0.5 ${sessionTheme.fontMonoSm} ${sessionTheme.info}`}
              >
                @{agent}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
