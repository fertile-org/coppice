import { describe, expect, it } from 'vitest';
import {
  chatIsolationBanner,
  chatUnavailableMessage,
  connectorDefaultModelLabel,
  readinessLabel,
  SIGN_IN_UNCHECKED_HINT,
  signInUncheckedHint,
} from './connectorCopy';

describe('connectorCopy helpers', () => {
  it('maps readiness, model label, and sign-in hints', () => {
    expect(readinessLabel('found')).toBe('Installed');
    expect(readinessLabel('ready')).toBe('Ready');
    expect(readinessLabel(null)).toBeNull();
    expect(connectorDefaultModelLabel('Cursor')).toBe("Cursor's default");
    expect(signInUncheckedHint('kilo-code', 'found')).toBe(SIGN_IN_UNCHECKED_HINT);
    expect(signInUncheckedHint('codex', 'found')).toBeNull();
    expect(signInUncheckedHint('kilo-code', 'ready')).toBeNull();
  });

  it('gates chat copy by connector id', () => {
    expect(chatIsolationBanner('claude-code')).toContain('read-only');
    expect(chatIsolationBanner('kilo-code')).toContain('read-only');
    for (const id of ['opencode', 'codex', 'cursor', 'mock', null] as const) {
      expect(chatIsolationBanner(id)).toBeNull();
    }
    expect(chatUnavailableMessage('cursor')).toBeNull();
    expect(chatUnavailableMessage('claude-code')).toBeNull();
    expect(chatUnavailableMessage('not-a-connector')).toContain('not-a-connector');
    for (const id of ['copilot', 'qwen-code', 'kimi', 'grok'] as const) {
      expect(chatIsolationBanner(id)).toBeNull();
      expect(chatUnavailableMessage(id)).toBeNull();
    }
    for (const id of ['copilot', 'qwen-code', 'kimi'] as const) {
      expect(signInUncheckedHint(id, 'found')).toBe(SIGN_IN_UNCHECKED_HINT);
    }
    expect(signInUncheckedHint('grok', 'found')).toBeNull();
  });
});
