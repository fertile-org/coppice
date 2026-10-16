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
    expect(chatIsolationBanner('codex')).toBe(
      "Coppice hasn't confirmed that Codex stays read-only. The agent can still run commands on your computer.",
    );
  });

  it('is absent when chat is off or the write test passed', () => {
    expect(chatIsolationBanner('cursor')).toBeNull();
    expect(chatIsolationBanner('kilo-code')).toBeNull();
    expect(chatIsolationBanner('opencode')).toBeNull();
    expect(chatIsolationBanner('mock')).toBeNull();
    expect(chatIsolationBanner(null)).toBeNull();
  });
});

describe('chatUnavailableMessage', () => {
  it('uses one locked sentence per CLI', () => {
    expect(chatUnavailableMessage('cursor')).toBe(
      "Chat isn't available for Cursor yet, because Coppice can't keep it read-only.",
    );
    expect(chatUnavailableMessage('kilo-code')).toBe(
      "Chat isn't available for Kilo Code yet, because Coppice can't keep it read-only.",
    );
    expect(chatUnavailableMessage('claude-code')).toBeNull();
    expect(chatUnavailableMessage('codex')).toBeNull();
    expect(chatUnavailableMessage('opencode')).toBeNull();
    expect(chatUnavailableMessage(null)).toBeNull();
  });
});
