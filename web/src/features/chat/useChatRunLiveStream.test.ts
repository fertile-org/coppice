import { describe, expect, it } from 'vitest';
import { createSessionStore } from '../../opencode-session/sync/reduce-event';
import type { ClaudeConsoleEntry } from '../runs/claude-console-state';
import {
  splitConsoleStream,
  splitLiveStreamChannels,
  splitOpenCodeStream,
} from './useChatRunLiveStream';

describe('splitConsoleStream', () => {
  it('puts thinking and pre-result text under thinking, not reply', () => {
    const entries: ClaudeConsoleEntry[] = [
      { kind: 'thinking', id: 't1', text: 'Checking updater…' },
      {
        kind: 'text',
        id: 'x1',
        markdown: 'Coppice already has electron-updater…',
      },
    ];
    expect(splitConsoleStream(entries)).toEqual({
      thinkingText: 'Checking updater…\n\nCoppice already has electron-updater…',
      replyText: '',
    });
  });

  it('uses result summary as reply and keeps earlier text as thinking', () => {
    const entries: ClaudeConsoleEntry[] = [
      { kind: 'text', id: 'x1', markdown: 'Drafting plan…' },
      {
        kind: 'result',
        id: 'r1',
        contract: {
          status: 'done',
          summary: 'Signed builds auto-update in place.',
        },
      },
      { kind: 'text', id: 'x2', markdown: 'Post-result note' },
    ];
    expect(splitConsoleStream(entries)).toEqual({
      thinkingText: 'Drafting plan…',
      replyText: 'Signed builds auto-update in place.',
    });
  });

  it('falls back to post-result text when summary is empty', () => {
    const entries: ClaudeConsoleEntry[] = [
      { kind: 'thinking', id: 't1', text: 'Hmm' },
      {
        kind: 'result',
        id: 'r1',
        contract: { status: 'done', summary: '' },
      },
      { kind: 'text', id: 'x1', markdown: 'More after result' },
    ];
    expect(splitConsoleStream(entries)).toEqual({
      thinkingText: 'Hmm',
      replyText: 'More after result',
    });
  });
});

describe('splitOpenCodeStream', () => {
  it('splits reasoning into thinking and text into reply', () => {
    const store = createSessionStore('s1');
    store.messages.push({
      id: 'm1',
      sessionID: 's1',
      role: 'assistant',
    });
    store.parts.m1 = [
      { id: 'p1', type: 'reasoning', text: 'Consider updater paths', messageID: 'm1' },
      { id: 'p2', type: 'text', text: 'Use electron-updater.', messageID: 'm1' },
    ];
    expect(splitOpenCodeStream(store)).toEqual({
      thinkingText: 'Consider updater paths',
      replyText: 'Use electron-updater.',
    });
  });
});

describe('splitLiveStreamChannels', () => {
  it('prefers console channels when console has content', () => {
    const store = createSessionStore('s1');
    store.messages.push({
      id: 'm1',
      sessionID: 's1',
      role: 'assistant',
    });
    store.parts.m1 = [
      { id: 'p2', type: 'text', text: 'OpenCode reply', messageID: 'm1' },
    ];
    const entries: ClaudeConsoleEntry[] = [
      { kind: 'text', id: 'x1', markdown: 'Console progress' },
      {
        kind: 'result',
        id: 'r1',
        contract: { status: 'done', summary: 'Console summary' },
      },
    ];
    expect(splitLiveStreamChannels(store, entries)).toEqual({
      thinkingText: 'Console progress',
      replyText: 'Console summary',
    });
  });
});
