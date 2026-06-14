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

function clonePart(part: Part): Part {
  if (part.type === 'tool') {
    return { ...part, state: { ...part.state } };
  }
  return { ...part };
}

/** Deep-enough clone for reducer passes (avoids StrictMode double-apply mutation). */
export function cloneSessionStore(store: SessionStore): SessionStore {
  const next: SessionStore = {
    sessionId: store.sessionId,
    messages: store.messages.map((message) => ({ ...message })),
    parts: Object.fromEntries(
      Object.entries(store.parts).map(([messageId, parts]) => [
        messageId,
        parts.map(clonePart),
      ]),
    ),
    pendingDeltas: Object.fromEntries(
      Object.entries(store.pendingDeltas).map(([partId, deltas]) => [
        partId,
        deltas.map((delta) => ({ ...delta })),
      ]),
    ),
  };
  rebuildPartLocations(next);
  return next;
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
  store.messages = snapshot.messages.map((message) => ({ ...message }));
  store.parts = Object.fromEntries(
    Object.entries(snapshot.parts).map(([messageId, parts]) => [
      messageId,
      parts.map(clonePart),
    ]),
  );
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
      const updated = applyFieldDelta(part, field, delta);
      if (updated !== part) {
        const nextParts = [...parts];
        nextParts[index] = updated;
        store.parts[messageId] = nextParts;
      }
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
    store.messages = store.messages.map((existing, i) =>
      i === index ? next : existing,
    );
  } else {
    store.messages = [...store.messages, next];
  }
}

function upsertPart(store: SessionStore, messageId: string, incoming: Part): void {
  const partId = incoming.id;
  const parts = store.parts[messageId] ?? [];
  const index = parts.findIndex((part) => part.id === partId);
  const locations = getPartLocations(store);

  if (index >= 0) {
    const merged = mergePart(parts[index], incoming);
    const nextParts = [...parts];
    nextParts[index] = merged;
    store.parts[messageId] = nextParts;
    locations[partId] = [messageId, index];
    return;
  }

  const nextParts = [...parts, incoming];
  store.parts[messageId] = nextParts;
  locations[partId] = [messageId, nextParts.length - 1];
}

function replayPendingDeltas(store: SessionStore, partId: string): void {
  const deltas = store.pendingDeltas[partId];
  if (!deltas) return;
  delete store.pendingDeltas[partId];

  const locations = getPartLocations(store);
  const location = locations[partId];
  if (!location) return;

  const [messageId, index] = location;
  const parts = store.parts[messageId];
  const part = parts?.[index];
  if (!part) return;

  let updated = part;
  for (const pending of deltas) {
    updated = applyFieldDelta(updated, pending.field, pending.delta);
  }
  if (updated !== part) {
    const nextParts = [...parts];
    nextParts[index] = updated;
    store.parts[messageId] = nextParts;
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
  if (
    incoming.type !== 'text' &&
    incoming.type !== 'reasoning' &&
    incoming.type !== 'compaction'
  ) {
    return incoming;
  }

  const existingText = 'text' in existing ? existing.text : '';
  const incomingText = incoming.text;
  if (incomingText === existingText) {
    return incoming;
  }
  if (incomingText.startsWith(existingText)) {
    return incoming;
  }
  if (existingText.startsWith(incomingText)) {
    return { ...incoming, text: existingText };
  }
  if ([...existingText].length > [...incomingText].length) {
    return { ...incoming, text: existingText };
  }
  return incoming;
}

/** Pure merge for streaming field deltas (text / reasoning). */
export function mergeFieldDelta(current: string, delta: string): string {
  if (!delta) return current;
  if (delta === current) return current;
  if (current && delta.startsWith(current)) return delta;
  if (current && current.endsWith(delta)) return current;
  return `${current}${delta}`;
}

function applyFieldDelta(part: Part, field: string, delta: string): Part {
  const record = part as Part & Record<string, unknown>;
  const current = typeof record[field] === 'string' ? record[field] : '';
  const next = mergeFieldDelta(current, delta);
  if (next === current) return part;
  return { ...part, [field]: next } as Part;
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
