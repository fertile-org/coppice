import { describe, expect, it } from 'vitest';
import { stepDrawerSection } from './drawerSections';

describe('drawer sections', () => {
  it('moves across Detail, Live Console, and Agent Runs without wrapping', () => {
    expect(stepDrawerSection('detail', 1)).toBe('live');
    expect(stepDrawerSection('live', 1)).toBe('runs');
    expect(stepDrawerSection('runs', 1)).toBe('runs');
    expect(stepDrawerSection('runs', -1)).toBe('live');
    expect(stepDrawerSection('detail', -1)).toBe('detail');
  });
});
