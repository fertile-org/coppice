import { describe, expect, it } from 'vitest';
import {
  chatIsolationBanner,
  chatUnavailableMessage,
  CONNECTOR_DEFAULT_MODEL_LABEL,
  connectorDefaultModelLabel,
  readinessLabel,
  SIGN_IN_UNCHECKED_HINT,
  signInUncheckedHint,
} from './connectorCopy';

describe('readinessLabel', () => {
  it('names a found CLI Installed', () => {
    expect(readinessLabel('found')).toBe('Installed');
  });

  it('adds the plan hint only for a connector with no free sign-in check', () => {
    expect(signInUncheckedHint('kilo-code', 'found')).toBe(SIGN_IN_UNCHECKED_HINT);
    expect(signInUncheckedHint('codex', 'found')).toBeNull();
    expect(signInUncheckedHint('kilo-code', 'ready')).toBeNull();
  });

  it('keeps the other readiness labels', () => {
    expect(readinessLabel('ready')).toBe('Ready');
    expect(readinessLabel('found_not_signed_in')).toBe('Found, not signed in');
    expect(readinessLabel('not_on_path')).toBe('Not on your PATH');
    expect(readinessLabel(null)).toBeNull();
    expect(readinessLabel(undefined)).toBeNull();
  });
});

describe('connectorDefaultModelLabel', () => {
  it('fills the connector display name into the template', () => {
    expect(CONNECTOR_DEFAULT_MODEL_LABEL).toBe("{connector}'s default");
    expect(connectorDefaultModelLabel('Cursor')).toBe("Cursor's default");
    expect(connectorDefaultModelLabel('Claude Code')).toBe("Claude Code's default");
  });
});

describe('chatIsolationBanner', () => {
  it('warns for each CLI that has not been shown to stay read-only', () => {
    expect(chatIsolationBanner('claude-code')).toBe(
      "Coppice hasn't confirmed that Claude Code stays read-only. The agent can still run commands on your computer.",
    );
    expect(chatIsolationBanner('cursor')).toBe(
      "Coppice hasn't confirmed that Cursor stays read-only. The agent can still run commands on your computer.",
    );
    expect(chatIsolationBanner('kilo-code')).toBe(
      "Coppice hasn't confirmed that Kilo Code stays read-only. The agent can still run commands on your computer.",
    );
  });

  it('is absent when the write test passed', () => {
    expect(chatIsolationBanner('opencode')).toBeNull();
    expect(chatIsolationBanner('codex')).toBeNull();
    expect(chatIsolationBanner('mock')).toBeNull();
    expect(chatIsolationBanner(null)).toBeNull();
  });
});

describe('chatUnavailableMessage', () => {
  it('is absent for every chat CLI', () => {
    expect(chatUnavailableMessage('cursor')).toBeNull();
    expect(chatUnavailableMessage('kilo-code')).toBeNull();
    expect(chatUnavailableMessage('claude-code')).toBeNull();
    expect(chatUnavailableMessage('codex')).toBeNull();
    expect(chatUnavailableMessage('opencode')).toBeNull();
    expect(chatUnavailableMessage(null)).toBeNull();
  });

  it('still refuses an unknown connector', () => {
    expect(chatUnavailableMessage('not-a-connector')).toBe(
      "Chat isn't available for not-a-connector yet, because Coppice can't keep it read-only.",
    );
  });
});

describe('ACP CLI connectors', () => {
  it.each(['copilot', 'qwen-code', 'kimi', 'grok'])('%s chats without the read-only banner', (id) => {
    expect(chatIsolationBanner(id)).toBeNull();
    expect(chatUnavailableMessage(id)).toBeNull();
  });

  it.each(['copilot', 'qwen-code', 'kimi'])('%s says its sign-in is unchecked when installed', (id) => {
    expect(signInUncheckedHint(id, 'found')).toBe(SIGN_IN_UNCHECKED_HINT);
    expect(signInUncheckedHint(id, 'ready')).toBeNull();
  });

  it('checks Grok sign-in with grok models, so it has no unchecked hint', () => {
    expect(signInUncheckedHint('grok', 'found')).toBeNull();
  });
});
