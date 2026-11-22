import { describe, expect, it } from 'vitest';
import { applyEvent, createSessionStore, nextSessionStore } from './reduce-event';
import type { TextPart } from './types';

describe('applyEvent', () => {
  it('appends text deltas incrementally', () => {
    const store = createSessionStore('ses_1');
    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'hello ',
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'world',
      },
    });
    expect((store.parts['msg_1'][0] as TextPart).text).toBe('hello world');
  });

  it('handles early deltas and skips duplicate full-text deltas', () => {
    const early = createSessionStore('ses_1');
    applyEvent(early, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'early',
      },
    });
    applyEvent(early, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    expect((early.parts['msg_1'][0] as TextPart).text).toBe('early');

    const store = createSessionStore('ses_1');
    const payload = JSON.stringify({
      status: 'done',
      summary: 'Done once.',
      nextStatus: 'Done',
    });

    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: payload,
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: payload,
      },
    });
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: payload,
      },
    });

    expect((store.parts['msg_1'][0] as TextPart).text).toBe(payload);
  });

});

describe('nextSessionStore', () => {
  it('applies a delta without touching the previous store', () => {
    const base = createSessionStore('ses_1');
    applyEvent(base, {
      type: 'message.updated',
      properties: { sessionID: 'ses_1', info: { id: 'msg_1', role: 'assistant' } },
    });
    applyEvent(base, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: 'a', messageID: 'msg_1' },
      },
    });
    const next = nextSessionStore(base);
    applyEvent(next, {
      type: 'message.part.delta',
      properties: { sessionID: 'ses_1', partID: 'prt_1', field: 'text', delta: 'b' },
    });
    expect((next.parts.msg_1[0] as TextPart).text).toBe('ab');
    expect((base.parts.msg_1[0] as TextPart).text).toBe('a');
    // Untouched messages are shared, not copied.
    expect(next.messages).toBe(base.messages);

    // A delta that arrives before its part buffers without changing the old store.
    const later = nextSessionStore(next);
    applyEvent(later, {
      type: 'message.part.delta',
      properties: { sessionID: 'ses_1', partID: 'prt_2', field: 'text', delta: 'early' },
    });
    expect(later.pendingDeltas.prt_2).toHaveLength(1);
    expect(next.pendingDeltas.prt_2).toBeUndefined();
  });
});
