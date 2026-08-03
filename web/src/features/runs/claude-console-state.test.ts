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

  it('coalesces Codex command lifecycle events into one shell card', () => {
    let state = applyClaudeConsoleEvent(createClaudeConsoleState(), {
      type: 'codex.console.tool',
      id: 'cmd_1',
      variant: 'shell',
      status: 'running',
      title: 'cargo test',
    });
    state = applyClaudeConsoleEvent(state, {
      type: 'codex.console.tool',
      id: 'cmd_1',
      variant: 'shell',
      status: 'completed',
      title: 'cargo test',
      output: 'test result: ok',
    });

    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({
      kind: 'tool',
      toolId: 'cmd_1',
      variant: 'shell',
      status: 'completed',
      title: 'cargo test',
      output: 'test result: ok',
    });
  });

  it('replays ordered Codex activity without duplicate tool rows', () => {
    const events = [
      {
        type: 'codex.console.thinking',
        text: 'Inspecting the provider boundary.',
      },
      {
        type: 'codex.console.tool',
        id: 'cmd_1',
        variant: 'shell',
        status: 'running',
        title: 'cargo test',
      },
      {
        type: 'codex.console.tool',
        id: 'cmd_1',
        variant: 'shell',
        status: 'completed',
        title: 'cargo test',
        output: 'ok',
      },
      {
        type: 'codex.console.tool',
        id: 'patch_1',
        variant: 'action',
        status: 'completed',
        title: 'File changes',
        output: 'update server/src/providers/codex_console.rs',
      },
      {
        type: 'codex.console.result',
        contract: {
          status: 'done',
          summary: 'Codex activity restored.',
          changedFiles: ['server/src/providers/codex_console.rs'],
          testsRun: ['cargo test'],
          blockers: [],
        },
      },
    ];

    const state = events.reduce(
      (current, event) => applyClaudeConsoleEvent(current, event),
      createClaudeConsoleState(),
    );

    expect(state.entries.map((entry) => entry.kind)).toEqual([
      'thinking',
      'tool',
      'tool',
      'result',
    ]);
    expect(state.entries[1]).toMatchObject({
      kind: 'tool',
      toolId: 'cmd_1',
      status: 'completed',
      output: 'ok',
    });
    expect(state.entries[2]).toMatchObject({
      kind: 'tool',
      toolId: 'patch_1',
      variant: 'action',
      status: 'completed',
    });
  });
});
