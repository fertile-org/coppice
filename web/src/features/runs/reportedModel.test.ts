import { describe, expect, it } from 'vitest';
import {
  MODEL_NOT_REPORTED,
  modelsListFailed,
  reportedModelLabel,
} from './reportedModel';

describe('reportedModelLabel', () => {
  it('shows the CLI string unchanged', () => {
    expect(reportedModelLabel('composer-2.5')).toBe('composer-2.5');
    expect(reportedModelLabel('  claude-sonnet-4-20250514  ')).toBe(
      'claude-sonnet-4-20250514',
    );
  });

  it('names the connector when its model list fails', () => {
    expect(modelsListFailed('Cursor')).toBe("Couldn't get Cursor's models.");
  });

  it('does not invent a model when the CLI reported none', () => {
    expect(reportedModelLabel(null)).toBe(MODEL_NOT_REPORTED);
    expect(reportedModelLabel(undefined)).toBe(MODEL_NOT_REPORTED);
    expect(reportedModelLabel('   ')).toBe(MODEL_NOT_REPORTED);
  });
});
