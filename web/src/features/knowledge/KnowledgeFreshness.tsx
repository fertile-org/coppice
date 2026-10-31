import { ChevronDown, ChevronRight, Flag } from 'lucide-react';
import { useState } from 'react';
import type { KnowledgeFreshness as Freshness } from '../../lib/schemas/knowledge';
import { shortId } from './knowledgeFormat';
import { useKnowledgeFreshness } from './useKnowledge';

const STATE_LABELS: Record<Freshness['state'], string> = {
  current: 'Current',
  changed: 'Changed',
  missing: 'Missing',
  unknown: 'Unknown',
  uncited: 'No citations',
};

const STATE_CLASSES: Record<Freshness['state'], string> = {
  current: 'text-moss-700',
  changed: 'text-warning',
  missing: 'text-danger',
  unknown: 'text-text-muted',
  uncited: 'text-text-muted',
};

/** Agents marked this item outdated or wrong. It stays approved until a reviewer acts. */
export function FlaggedByAgents({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-warning-muted px-2 py-0.5 font-body text-xs font-medium text-warning">
      <Flag className="size-3" aria-hidden="true" />
      Flagged by agents ({count})
    </span>
  );
}

function FreshnessDetails({ itemId }: { itemId: string }) {
  const freshness = useKnowledgeFreshness(itemId);

  if (freshness.isLoading) {
    return (
      <p className="mt-2 font-body text-xs text-text-muted">Checking the cited code…</p>
    );
  }
  if (freshness.isError || !freshness.data) {
    return (
      <p className="mt-2 font-body text-xs text-danger">Unable to check the cited code.</p>
    );
  }
  const { state, citations } = freshness.data;
  if (citations.length === 0) {
    return (
      <p className="mt-2 font-body text-xs text-text-secondary">This item cites no code.</p>
    );
  }
  return (
    <div className="mt-2">
      <p className="font-body text-xs text-text-secondary">
        Against the default branch:{' '}
        <span
          data-testid="freshness-state"
          className={`font-medium ${STATE_CLASSES[state]}`}
        >
          {STATE_LABELS[state]}
        </span>
      </p>
      <ul className="mt-1.5 space-y-1">
        {citations.map((citation) => (
          <li
            key={`${citation.repoName ?? ''}:${citation.path}`}
            className="font-mono text-xs text-text-secondary"
          >
            <span>{citation.path}</span>
            {citation.repoName && (
              <span className="text-text-muted"> in {citation.repoName}</span>
            )}
            {citation.commitSha && (
              <span className="text-text-muted"> at {shortId(citation.commitSha)}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Collapsed by default: the server runs git to answer, so it loads on expand. */
export function KnowledgeFreshness({ itemId }: { itemId: string }) {
  const [open, setOpen] = useState(false);
  const Icon = open ? ChevronDown : ChevronRight;
  return (
    <div className="mt-3">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="inline-flex items-center gap-1 font-body text-xs font-medium text-moss-700 hover:underline"
      >
        <Icon className="size-3.5" aria-hidden="true" />
        Cited code
      </button>
      {open && <FreshnessDetails itemId={itemId} />}
    </div>
  );
}
