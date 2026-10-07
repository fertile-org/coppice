import { ApiError } from '../../lib/api';

export interface GitConflict {
  operation: 'merge' | 'rebase';
  baseBranch: string;
  files: string[];
  message: string;
  canAskAssignee: boolean;
  askLabel: string | null;
  unavailableReason: string | null;
  rereviewNote: string;
}

export function parseGitConflict(err: unknown): GitConflict | null {
  if (!(err instanceof ApiError)) {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(err.body);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== 'object') {
    return null;
  }
  const record = parsed as { message?: unknown; conflict?: unknown };
  const conflict = record.conflict;
  if (!conflict || typeof conflict !== 'object' || typeof record.message !== 'string') {
    return null;
  }
  const offer = conflict as Record<string, unknown>;
  if (offer.operation !== 'merge' && offer.operation !== 'rebase') {
    return null;
  }
  if (typeof offer.baseBranch !== 'string' || !Array.isArray(offer.files)) {
    return null;
  }
  if (typeof offer.canAskAssignee !== 'boolean' || typeof offer.rereviewNote !== 'string') {
    return null;
  }
  const files = offer.files.filter((file): file is string => typeof file === 'string');
  return {
    operation: offer.operation,
    baseBranch: offer.baseBranch,
    files,
    message: record.message,
    canAskAssignee: offer.canAskAssignee,
    askLabel: typeof offer.askLabel === 'string' ? offer.askLabel : null,
    unavailableReason:
      typeof offer.unavailableReason === 'string' ? offer.unavailableReason : null,
    rereviewNote: offer.rereviewNote,
  };
}
