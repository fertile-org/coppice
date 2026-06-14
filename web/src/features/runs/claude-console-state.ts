import type { AgentResultContract } from '../../opencode-session/parts/parse-result-contract';
import { parseResultContractFromText } from '../../opencode-session/parts/parse-result-contract';

export type ClaudeToolStatus = 'running' | 'completed' | 'error';

export type ClaudeConsoleEntry =
  | { kind: 'session'; id: string; model: string; connector?: 'claude' | 'codex' }
  | { kind: 'thinking'; id: string; text: string }
  | { kind: 'text'; id: string; markdown: string }
  | {
      kind: 'tool';
      id: string;
      toolId: string;
      variant: 'shell' | 'action';
      status: ClaudeToolStatus;
      title: string;
      output?: string;
    }
  | { kind: 'result'; id: string; contract: AgentResultContract }
  | { kind: 'continued'; id: string; summary: string; progressNote?: string };

export interface ClaudeConsoleState {
  entries: ClaudeConsoleEntry[];
  legacyText: string;
}

let entryCounter = 0;

function nextEntryId(): string {
  entryCounter += 1;
  return `claude-entry-${entryCounter}`;
}

export function createClaudeConsoleState(): ClaudeConsoleState {
  return { entries: [], legacyText: '' };
}

function contractFromValue(value: unknown): AgentResultContract | null {
  if (value == null) return null;
  if (typeof value === 'string') {
    return parseResultContractFromText(value);
  }
  try {
    return parseResultContractFromText(JSON.stringify(value));
  } catch {
    return null;
  }
}

function continuedSummary(value: Record<string, unknown>): ClaudeConsoleEntry | null {
  const summary = typeof value.summary === 'string' ? value.summary.trim() : '';
  if (!summary) return null;
  const progressNote =
    typeof value.progressNote === 'string' ? value.progressNote : undefined;
  return {
    kind: 'continued',
    id: nextEntryId(),
    summary,
    progressNote,
  };
}

export function applyClaudeConsoleEvent(
  state: ClaudeConsoleState,
  event: Record<string, unknown>,
): ClaudeConsoleState {
  const ty = event.type;
  if (typeof ty !== 'string') return state;

  switch (ty) {
    case 'claude.console.session':
    case 'codex.console.session': {
      const model =
        typeof event.model === 'string' && event.model.trim()
          ? event.model
          : 'unknown';
      const connector = ty.startsWith('codex.') ? 'codex' : 'claude';
      return {
        ...state,
        entries: [
          ...state.entries,
          { kind: 'session', id: nextEntryId(), model, connector },
        ],
      };
    }
    case 'claude.console.thinking': {
      const text = typeof event.text === 'string' ? event.text.trim() : '';
      if (!text) return state;
      return {
        ...state,
        entries: [...state.entries, { kind: 'thinking', id: nextEntryId(), text }],
      };
    }
    case 'claude.console.text':
    case 'codex.console.text': {
      const markdown =
        typeof event.markdown === 'string' ? event.markdown.trim() : '';
      if (!markdown) return state;
      return {
        ...state,
        entries: [...state.entries, { kind: 'text', id: nextEntryId(), markdown }],
      };
    }
    case 'claude.console.tool':
    case 'codex.console.tool':
      return applyToolEvent(state, event);
    case 'claude.console.result':
    case 'codex.console.result':
      return applyResultEvent(state, event);
    default:
      return state;
  }
}

function applyToolEvent(
  state: ClaudeConsoleState,
  event: Record<string, unknown>,
): ClaudeConsoleState {
  const toolId = typeof event.id === 'string' ? event.id : null;
  if (!toolId) return state;

  const status =
    event.status === 'completed' || event.status === 'error'
      ? event.status
      : 'running';
  const output =
    typeof event.output === 'string' && event.output.trim()
      ? event.output
      : undefined;

  const existingIndex = state.entries.findIndex(
    (entry) => entry.kind === 'tool' && entry.toolId === toolId,
  );

  if (existingIndex >= 0) {
    const existing = state.entries[existingIndex];
    if (existing.kind !== 'tool') return state;
    const nextEntries = [...state.entries];
    nextEntries[existingIndex] = {
      ...existing,
      status,
      output: output ?? existing.output,
      title:
        typeof event.title === 'string' && event.title.trim()
          ? event.title
          : existing.title,
      variant:
        event.variant === 'shell' || event.variant === 'action'
          ? event.variant
          : existing.variant,
    };
    return { ...state, entries: nextEntries };
  }

  const title = typeof event.title === 'string' ? event.title : '';
  const variant = event.variant === 'shell' ? 'shell' : 'action';
  return {
    ...state,
    entries: [
      ...state.entries,
      {
        kind: 'tool',
        id: nextEntryId(),
        toolId,
        variant,
        status,
        title,
        output,
      },
    ],
  };
}

function applyResultEvent(
  state: ClaudeConsoleState,
  event: Record<string, unknown>,
): ClaudeConsoleState {
  if (state.entries.some((entry) => entry.kind === 'result')) {
    return state;
  }

  const contract = contractFromValue(event.contract);
  if (contract) {
    return {
      ...state,
      entries: [
        ...state.entries,
        { kind: 'result', id: nextEntryId(), contract },
      ],
    };
  }

  if (
    typeof event.contract === 'object' &&
    event.contract !== null &&
    (event.contract as Record<string, unknown>).status === 'continued'
  ) {
    const continued = continuedSummary(event.contract as Record<string, unknown>);
    if (continued) {
      return { ...state, entries: [...state.entries, continued] };
    }
  }

  return state;
}

export function appendLegacyFrameText(
  state: ClaudeConsoleState,
  text: string,
): ClaudeConsoleState {
  if (!text) return state;
  return { ...state, legacyText: state.legacyText + text };
}

export function resetClaudeConsoleState(): ClaudeConsoleState {
  entryCounter = 0;
  return createClaudeConsoleState();
}
