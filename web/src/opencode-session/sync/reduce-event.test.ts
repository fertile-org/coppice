import { describe, expect, it } from 'vitest';
import { applyEvent, cloneSessionStore, createSessionStore } from './reduce-event';
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

  it('handles delta before part.updated', () => {
    const store = createSessionStore('ses_1');
    applyEvent(store, {
      type: 'message.part.delta',
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta: 'early',
      },
    });
    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'text', text: '', messageID: 'msg_1' },
      },
    });
    expect((store.parts['msg_1'][0] as TextPart).text).toBe('early');
  });

  it('skips duplicate full-text deltas', () => {
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

  it('re-applying the same delta on a cloned store does not duplicate text', () => {
    const store = createSessionStore('ses_1');
    applyEvent(store, {
      type: 'message.part.updated',
      properties: {
        sessionID: 'ses_1',
        part: { id: 'prt_1', type: 'reasoning', text: '', messageID: 'msg_1' },
      },
    });

    const deltaEvent = {
      type: 'message.part.delta' as const,
      properties: {
        sessionID: 'ses_1',
        partID: 'prt_1',
        field: 'text',
        delta:
          "I'm the Tech Lead Agent reviewing a ticket in in_review status. Let me investigate.",
      },
    };

    const working = cloneSessionStore(store);
    applyEvent(working, deltaEvent);
    const once = (working.parts['msg_1'][0] as TextPart).text;

    const strictModePass = cloneSessionStore(store);
    applyEvent(strictModePass, deltaEvent);
    expect((strictModePass.parts['msg_1'][0] as TextPart).text).toBe(once);
  });
});
