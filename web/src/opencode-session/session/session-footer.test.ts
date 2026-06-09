import { describe, expect, it } from 'vitest';
import { sessionModelInfo } from './session-footer';
import type { Message } from '../sync/types';

describe('sessionModelInfo', () => {
  it('returns null when there are no assistant messages', () => {
    expect(sessionModelInfo([])).toBeNull();
    expect(
      sessionModelInfo([{ id: '1', sessionID: 's', role: 'user' }]),
    ).toBeNull();
  });

  it('returns mode and model from assistant messages with total duration', () => {
    const messages: Message[] = [
      {
        id: '1',
        sessionID: 's',
        role: 'assistant',
        mode: 'build',
        modelID: 'glm-5.1',
        time: { created: 0, completed: 3000 },
      },
      {
        id: '2',
        sessionID: 's',
        role: 'assistant',
        mode: 'build',
        modelID: 'glm-5.1',
        time: { created: 3000, completed: 5000 },
      },
    ];

    expect(sessionModelInfo(messages)).toEqual({
      mode: 'build',
      modelID: 'glm-5.1',
      duration: '5s',
    });
  });
});
