import { describe, expect, it } from 'vitest';
import { MODEL_NOT_REPORTED, modelsListFailed, reportedModelLabel } from './reportedModel';

describe('reportedModelLabel', () => {
  it('labels reported models, failures, and gaps', () => {
    expect(reportedModelLabel('composer-2.5')).toBe('composer-2.5');
    expect(reportedModelLabel(null)).toBe(MODEL_NOT_REPORTED);
    expect(modelsListFailed('Cursor')).toContain('Refresh models');
  });
});