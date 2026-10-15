import { describe, expect, it } from 'vitest';
import {
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

describe('chatUnavailableMessage', () => {
  it('explains that Kilo Code chat is unavailable', () => {
    expect(chatUnavailableMessage('kilo-code')).toBe(
      "Chat isn't available for Kilo Code yet, because Chat runs read-only and Kilo Code can't.",
    );
  });

  it('explains chat is unavailable for each connector that cannot stay read-only', () => {
    expect(chatUnavailableMessage('codex')).toBe(
      "Chat isn't available for Codex yet, because Chat runs read-only and Codex can't.",
    );
    expect(chatUnavailableMessage('opencode')).toBe(
      "Chat isn't available for OpenCode yet, because Chat runs read-only and OpenCode can't.",
    );
    expect(chatUnavailableMessage('cursor')).toBe(
      "Chat isn't available for Cursor yet, because Chat runs read-only and Cursor can't.",
    );
  });

  it('leaves Claude Code available', () => {
    expect(chatUnavailableMessage('claude-code')).toBeNull();
    expect(chatUnavailableMessage(null)).toBeNull();
  });
});
