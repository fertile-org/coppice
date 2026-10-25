import { describe, expect, it } from 'vitest';
import { REJECT_PRESETS } from './curationGuide';

describe('curationGuide', () => {
  it('exposes reject presets with human-readable reasonText', () => {
    expect(REJECT_PRESETS.map((preset) => preset.id)).toEqual([
      'one_off',
      'too_vague',
      'wrong_scope',
      'duplicate',
    ]);
    for (const preset of REJECT_PRESETS) {
      expect(preset.label.length).toBeGreaterThan(0);
      expect(preset.reasonText).not.toBe(preset.id);
      expect(preset.reasonText.length).toBeGreaterThan(10);
    }
  });
});
