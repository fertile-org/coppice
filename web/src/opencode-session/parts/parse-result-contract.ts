export interface DoneResultContract {
  status: 'done';
  summary: string;
  changedFiles?: string[];
  testsRun?: string[];
  nextStatus?: string;
  mentionAgents?: string[];
  blockers?: string[];
}

export interface BlockedResultContract {
  status: 'blocked';
  blockerType?: string;
  summary: string;
  nextStatus?: string;
  mentionAgents?: string[];
  requiredCapabilities?: string[];
  requiredSecrets?: string[];
}

export type AgentResultContract = DoneResultContract | BlockedResultContract;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function stringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter((item): item is string => typeof item === 'string');
}

function parseContractObject(value: unknown): AgentResultContract | null {
  if (!isRecord(value)) return null;
  const status = value.status;
  const summary = typeof value.summary === 'string' ? value.summary : undefined;
  if (!summary) return null;

  if (status === 'done') {
    return {
      status: 'done',
      summary,
      changedFiles: stringArray(value.changedFiles),
      testsRun: stringArray(value.testsRun),
      nextStatus: typeof value.nextStatus === 'string' ? value.nextStatus : undefined,
      mentionAgents: stringArray(value.mentionAgents),
      blockers: stringArray(value.blockers),
    };
  }

  if (status === 'blocked') {
    return {
      status: 'blocked',
      summary,
      blockerType: typeof value.blockerType === 'string' ? value.blockerType : undefined,
      nextStatus: typeof value.nextStatus === 'string' ? value.nextStatus : undefined,
      mentionAgents: stringArray(value.mentionAgents),
      requiredCapabilities: stringArray(value.requiredCapabilities),
      requiredSecrets: stringArray(value.requiredSecrets),
    };
  }

  return null;
}

export function looksLikeTemplateContract(contract: AgentResultContract): boolean {
  return contract.summary.includes('<') && contract.summary.includes('>');
}

function tryParseJson(text: string): AgentResultContract | null {
  try {
    const value = JSON.parse(text.trim()) as unknown;
    const contract = parseContractObject(value);
    if (!contract || looksLikeTemplateContract(contract)) return null;
    return contract;
  } catch {
    return null;
  }
}

export function parseResultContractFromText(text: string): AgentResultContract | null {
  const trimmed = text.trim();
  if (!trimmed) return null;

  const direct = tryParseJson(trimmed);
  if (direct) return direct;

  const candidates: AgentResultContract[] = [];
  let searchFrom = 0;
  while (searchFrom < trimmed.length) {
    const rel = trimmed.indexOf('```json', searchFrom);
    if (rel === -1) break;
    const absStart = rel + 7;
    const after = trimmed.slice(absStart);
    const end = after.indexOf('```');
    if (end === -1) break;
    const contract = tryParseJson(after.slice(0, end));
    if (contract) candidates.push(contract);
    searchFrom = absStart + end + 3;
  }
  if (candidates.length > 0) {
    return candidates[candidates.length - 1];
  }

  for (const line of trimmed.split('\n').reverse()) {
    const candidate = line.trim();
    if (!candidate.startsWith('{')) continue;
    const contract = tryParseJson(candidate);
    if (contract) return contract;
  }

  return null;
}
