import { describe, expect, it } from 'vitest';
import {
  applyClaudeConsoleEvent,
  createClaudeConsoleState,
} from './claude-console-state';

describe('applyClaudeConsoleEvent', () => {
  it('accepts structured console text from any connector prefix', () => {
    const state = applyClaudeConsoleEvent(createClaudeConsoleState(), {
      type: 'kilo.console.text',
      markdown: 'Kilo output',
    });

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({
      kind: 'text',
      markdown: 'Kilo output',
    });
  });

  it('preserves the connector id on generic session events', () => {
    const state = applyClaudeConsoleEvent(createClaudeConsoleState(), {
      type: 'future-agent.console.session',
      model: 'model-x',
    });

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({
      kind: 'session',
      model: 'model-x',
      connector: 'future-agent',
    });
  });

  it('ignores non-console events', () => {
    const state = applyClaudeConsoleEvent(createClaudeConsoleState(), {
      type: 'session.message',
      markdown: 'ignored',
    });

    expect(state.entries).toEqual([]);
  });
});
