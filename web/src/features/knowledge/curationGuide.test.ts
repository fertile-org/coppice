import { describe, expect, it } from 'vitest';
import {
  CURATION_LITMUS,
  CURATION_TRUST_FRAMING,
  REJECT_PRESETS,
  guidanceForType,
} from './curationGuide';

describe('curationGuide', () => {
  it('exports the reuse litmus and trust framing', () => {
    expect(CURATION_LITMUS).toBe(
      'Would a different ticket next month still need this exact rule?',
    );
    expect(CURATION_TRUST_FRAMING).toMatch(/eligible for retrieval/);
    expect(CURATION_TRUST_FRAMING).toMatch(/not instruction authority/);
  });

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

  it('returns type-aware guidance and a shared fallback', () => {
    const typed = guidanceForType('test_command');
    expect(typed.approveExample).toMatch(/test-unit/i);
    expect(typed.rejectExample).toMatch(/Reject:/);

    const fallback = guidanceForType('unknown_type');
    expect(fallback.approveExample).toMatch(/durable/i);
    expect(fallback.rejectExample).toMatch(/Reject:/);

    expect(guidanceForType(null)).toEqual(fallback);
    expect(guidanceForType(undefined)).toEqual(fallback);
  });
});
