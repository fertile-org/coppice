export type PartType = 'text' | 'reasoning' | 'tool' | 'compaction';

export interface TextPart {
  id: string;
  type: 'text';
  text: string;
  messageID: string;
}

export interface ReasoningPart {
  id: string;
  type: 'reasoning';
  text: string;
  messageID: string;
}

export interface CompactionPart {
  id: string;
  type: 'compaction';
  text: string;
  messageID: string;
  auto?: boolean;
}

export interface ToolPart {
  id: string;
  type: 'tool';
  tool: string;
  callID?: string;
  messageID: string;
  state: {
    status: string;
    input?: Record<string, unknown>;
    output?: unknown;
    metadata?: Record<string, unknown>;
  };
}

export type Part = TextPart | ReasoningPart | CompactionPart | ToolPart;

export interface Message {
  id: string;
  sessionID: string;
  role: 'user' | 'assistant';
  time?: { created?: number; completed?: number };
  parentID?: string;
  modelID?: string;
  mode?: string;
  agent?: string;
  finish?: string;
  error?: { name?: string; data?: { message?: string } };
}

export interface SessionStore {
  sessionId: string;
  messages: Message[];
  parts: Record<string, Part[]>;
  pendingDeltas: Record<string, Array<{ field: string; delta: string }>>;
}

export interface SessionSnapshot {
  sessionId: string;
  messages: Message[];
  parts: Record<string, Part[]>;
}

export interface OpenCodeEvent {
  type: string;
  properties?: Record<string, unknown>;
}
