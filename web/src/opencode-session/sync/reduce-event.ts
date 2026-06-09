import type {
  Message,
  OpenCodeEvent,
  Part,
  SessionSnapshot,
  SessionStore,
} from './types';

const partLocations = new WeakMap<SessionStore, Record<string, [string, number]>>();

function getPartLocations(store: SessionStore): Record<string, [string, number]> {
  let locations = partLocations.get(store);
  if (!locations) {
    locations = {};
    partLocations.set(store, locations);
  }
  return locations;
}

function rebuildPartLocations(store: SessionStore): void {
  const locations: Record<string, [string, number]> = {};
  for (const [messageId, parts] of Object.entries(store.parts)) {
    parts.forEach((part, index) => {
      locations[part.id] = [messageId, index];
    });
  }
  partLocations.set(store, locations);
}

export function createSessionStore(sessionId: string): SessionStore {
  const store: SessionStore = {
    sessionId,
    messages: [],
    parts: {},
    pendingDeltas: {},
  };
  partLocations.set(store, {});
  return store;
}

export function applySnapshot(store: SessionStore, snapshot: SessionSnapshot): void {
  store.sessionId = snapshot.sessionId;
  store.messages = snapshot.messages;
  store.parts = snapshot.parts;
  store.pendingDeltas = {};
  rebuildPartLocations(store);
}

export function applyEvent(store: SessionStore, event: OpenCodeEvent): void {
  switch (event.type) {
    case 'message.part.updated':
      applyPartUpdated(store, event);
      break;
    case 'message.part.delta':
      applyPartDelta(store, event);
      break;
    case 'message.updated':
      applyMessageUpdated(store, event);
      break;
    default:
      break;
  }
}

function applyPartDelta(store: SessionStore, event: OpenCodeEvent): void {
  const props = event.properties;
  if (!props) return;
  if (props.sessionID !== store.sessionId) return;

  const partId = asString(props.partID);
  const field = asString(props.field);
  const delta = asString(props.delta);
  if (!partId || !field || delta === undefined) return;

  const locations = getPartLocations(store);
  const location = locations[partId];
  if (location) {
    const [messageId, index] = location;
    const parts = store.parts[messageId];
    const part = parts?.[index];
    if (part) {
      appendFieldDelta(part, field, delta);
      return;
    }
    delete locations[partId];
  }

  const pending = store.pendingDeltas[partId] ?? [];
  pending.push({ field, delta });
  store.pendingDeltas[partId] = pending;
}

function applyPartUpdated(store: SessionStore, event: OpenCodeEvent): void {
  const props = event.properties;
  if (!props) return;
  if (props.sessionID !== store.sessionId) return;

  const part = props.part;
  if (!isRecord(part)) return;

  const partId = asString(part.id);
  if (!partId) return;

  const messageId =
    asString(part.messageID) ?? asString(part.messageId) ?? 'unknown';

  upsertPart(store, messageId, part as unknown as Part);
  replayPendingDeltas(store, partId);
}

function applyMessageUpdated(store: SessionStore, event: OpenCodeEvent): void {
  const props = event.properties;
  if (!props) return;
  if (props.sessionID !== store.sessionId) return;

  const message = props.message ?? props.info;
  if (!isRecord(message)) return;

  const messageId = messageIdFromValue(message);
  if (!messageId) return;

  const index = store.messages.findIndex((existing) => existing.id === messageId);
  const next = message as unknown as Message;
  if (index >= 0) {
    store.messages[index] = next;
  } else {
    store.messages.push(next);
  }
}

function upsertPart(store: SessionStore, messageId: string, incoming: Part): void {
  const partId = incoming.id;
  const parts = store.parts[messageId] ?? [];
  const index = parts.findIndex((part) => part.id === partId);
  const locations = getPartLocations(store);

  if (index >= 0) {
    parts[index] = mergePart(parts[index], incoming);
    store.parts[messageId] = parts;
    locations[partId] = [messageId, index];
    return;
  }

  const position = parts.length;
  parts.push(incoming);
  store.parts[messageId] = parts;
  locations[partId] = [messageId, position];
}

function replayPendingDeltas(store: SessionStore, partId: string): void {
  const deltas = store.pendingDeltas[partId];
  if (!deltas) return;
  delete store.pendingDeltas[partId];

  const locations = getPartLocations(store);
  const location = locations[partId];
  if (!location) return;

  const [messageId, index] = location;
  const part = store.parts[messageId]?.[index];
  if (!part) return;

  for (const pending of deltas) {
    appendFieldDelta(part, pending.field, pending.delta);
  }
}

function messageIdFromValue(message: Record<string, unknown>): string | undefined {
  const direct = asString(message.id);
  if (direct) return direct;

  const info = message.info;
  if (!isRecord(info)) return undefined;
  return asString(info.id);
}

function mergePart(existing: Part, incoming: Part): Part {
  if (incoming.type !== 'text' && incoming.type !== 'reasoning') {
    return incoming;
  }

  const existingText = 'text' in existing ? existing.text : '';
  const incomingText = incoming.text;
  if ([...existingText].length > [...incomingText].length) {
    return { ...incoming, text: existingText };
  }
  return incoming;
}

function appendFieldDelta(part: Part, field: string, delta: string): void {
  const record = part as Part & Record<string, unknown>;
  const current = typeof record[field] === 'string' ? record[field] : '';
  record[field] = `${current}${delta}`;
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
