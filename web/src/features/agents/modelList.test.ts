import { describe, expect, it } from 'vitest';
import {
  CURSOR_MODELS_SIGN_IN,
  connectorModelsPath,
  decodeModelValue,
  encodeModelValue,
  modelsFailureExtra,
  modelsUnavailableMessage,
  turnedOffConnectorName,
} from './modelList';

describe('model list copy', () => {
  it('locks the failure message', () => {
    expect(modelsUnavailableMessage('Cursor')).toBe(
      "Couldn't get Cursor's models. Using Cursor's default for now.",
    );
    expect(modelsUnavailableMessage('Claude Code')).toBe(
      "Couldn't get Claude Code's models. Using Claude Code's default for now.",
    );
  });

  it('shows the stderr line, and the Cursor sign-in line when the CLI says so', () => {
    expect(modelsFailureExtra('kilo-code', 'not logged in')).toBe('not logged in');
    expect(modelsFailureExtra('cursor', 'boom')).toBe('boom');
    expect(modelsFailureExtra('cursor', 'Error: Authentication required')).toBe(
      CURSOR_MODELS_SIGN_IN,
    );
    expect(CURSOR_MODELS_SIGN_IN).toBe(
      "Cursor's CLI isn't signed in for Coppice. Run `agent login` in a terminal, then click Refresh models.",
    );
    expect(modelsFailureExtra('cursor', '  ')).toBeNull();
  });

  it('names the connector from the turned-off log line', () => {
    expect(
      turnedOffConnectorName(
        "Can't list Cursor's models because it's turned off in Connectors.",
      ),
    ).toBe('Cursor');
    expect(turnedOffConnectorName("Couldn't get Cursor's models.")).toBeNull();
  });

  it('requests models for the cursor provider and can bypass the cache', () => {
    expect(connectorModelsPath('cursor', 'cursor', false)).toBe(
      '/api/connectors/cursor/model-providers/cursor/models',
    );
    expect(connectorModelsPath('cursor', 'cursor', true)).toBe(
      '/api/connectors/cursor/model-providers/cursor/models?refresh=true',
    );
  });

  it('round-trips provider and model', () => {
    const encoded = encodeModelValue('~openai', 'gpt-4o');
    expect(decodeModelValue(encoded)).toEqual({ provider: '~openai', model: 'gpt-4o' });
  });
});
