import { ApiError } from '../../lib/api';

export interface ConflictAgent {
  id: string;
  name: string;
  askLabel: string;
  unavailableReason: string | null;
}

export interface GitConflict {
  operation: 'merge' | 'rebase';
  baseBranch: string;
  files: string[];
  message: string;
  canAskAssignee: boolean;
  askLabel: string | null;
  unavailableReason: string | null;
  defaultAgentId: string | null;
  agents: ConflictAgent[];
  rereviewNote: string;
}

function parseConflictAgent(value: unknown): ConflictAgent | null {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const agent = value as Record<string, unknown>;
  if (
    typeof agent.id !== 'string' ||
    typeof agent.name !== 'string' ||
    typeof agent.askLabel !== 'string'
  ) {
    return null;
  }
  return {
    id: agent.id,
    name: agent.name,
    askLabel: agent.askLabel,
    unavailableReason:
      typeof agent.unavailableReason === 'string' ? agent.unavailableReason : null,
  };
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
  const agents = Array.isArray(offer.agents)
    ? offer.agents.flatMap((agent) => {
        const parsedAgent = parseConflictAgent(agent);
        return parsedAgent ? [parsedAgent] : [];
      })
    : [];
  return {
    operation: offer.operation,
    baseBranch: offer.baseBranch,
    files,
    message: record.message,
    canAskAssignee: offer.canAskAssignee,
    askLabel: typeof offer.askLabel === 'string' ? offer.askLabel : null,
    unavailableReason:
      typeof offer.unavailableReason === 'string' ? offer.unavailableReason : null,
    defaultAgentId: typeof offer.defaultAgentId === 'string' ? offer.defaultAgentId : null,
    agents,
    rereviewNote: offer.rereviewNote,
  };
}

export function selectedConflictAgent(
  conflict: GitConflict,
  selectedAgentId: string | null,
): ConflictAgent | null {
  if (conflict.agents.length === 0) {
    return null;
  }
  return (
    conflict.agents.find((agent) => agent.id === selectedAgentId) ??
    conflict.agents.find((agent) => agent.id === conflict.defaultAgentId) ??
    conflict.agents[0]
  );
}
