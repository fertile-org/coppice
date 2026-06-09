import { describe, expect, it } from 'vitest';
import { applyEvent, createSessionStore } from './reduce-event';

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
    expect(store.parts['msg_1'][0].text).toBe('hello world');
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
    expect(store.parts['msg_1'][0].text).toBe('early');
  });
});
