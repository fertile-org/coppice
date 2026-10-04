import type { ComboboxOption } from '../../components/ui/combobox';
import {
  knowledgeTypeSchema,
  type KnowledgeItem,
  type KnowledgeStatus,
  type KnowledgeType,
} from '../../lib/schemas/knowledge';
import { REJECT_PRESETS } from './curationGuide';

export const TYPE_LABELS: Record<KnowledgeType, string> = {
  coding_convention: 'Coding convention',
  architecture_rule: 'Architecture rule',
  bug_pattern: 'Bug pattern',
  test_command: 'Test command',
  review_feedback: 'Review feedback',
  dependency_note: 'Dependency note',
  api_contract: 'API contract',
  workflow_rule: 'Workflow rule',
  human_preference: 'Human preference',
  operational_runbook: 'Operational runbook',
  security_rule: 'Security rule',
  performance_note: 'Performance note',
};

export const KNOWLEDGE_TYPES = knowledgeTypeSchema.options;

export const TYPE_OPTIONS: ComboboxOption[] = KNOWLEDGE_TYPES.map((type) => ({
  value: type,
  label: TYPE_LABELS[type],
}));

export const CONFIDENCE_OPTIONS: ComboboxOption[] = [
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
];

export function formatDate(value: string | null): string {
  if (!value) return 'Never';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

export function humanize(value: string): string {
  return value
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function shortId(value: string): string {
  return value.slice(0, 8);
}

export function duplicateRejectReason(neighborTitle: string, neighborId: string): string {
  const preset = REJECT_PRESETS.find((entry) => entry.id === 'duplicate')!;
  return `${preset.reasonText.slice(0, -1)}: "${neighborTitle}" (${shortId(neighborId)}).`;
}

export function scopeLabel(item: KnowledgeItem): string {
  if (item.scope === 'workspace') return 'Workspace';
  if (item.scope === 'agent') {
    return `${item.boardName ?? 'Board'} · ${item.agentName ?? 'Agent'}`;
  }
  return item.boardName ?? 'Board';
}

export function statusPillClass(status: KnowledgeStatus): string {
  const base = 'rounded-full border px-2 py-0.5 font-body text-xs font-medium';
  switch (status) {
    case 'pending':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
    case 'approved':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'rejected':
      return `${base} border-danger-muted bg-danger-muted text-danger`;
    case 'stale':
      return `${base} border-border bg-paper-200 text-text-secondary`;
  }
}
