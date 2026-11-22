import { describe, expect, it } from 'vitest';
import { sectionForSet, splitPlanMarkers, stripDesignMarkers } from './designMarkers';

const SET = '11111111-1111-4111-8111-111111111111';

describe('designMarkers', () => {
  it('splits markers out of the plan', () => {
    const plan = `## Approach\nGo.\n\n## UI design\n\n{{design:${SET}}}\n\n### Steps\n- [ ] Ship`;
    expect(splitPlanMarkers(plan)).toEqual([
      { type: 'text', text: '## Approach\nGo.\n\n## UI design\n\n' },
      { type: 'design', setId: SET },
      { type: 'text', text: '\n\n### Steps\n- [ ] Ship' },
    ]);
  });

  it('strips markers for approved views', () => {
    const out = stripDesignMarkers(`## UI design\n\n{{design:${SET}}}\n\n### Steps`);
    expect(out).not.toContain('{{design:');
    expect(out).toContain('## UI design');
  });

  it('reads the nearest ## heading', () => {
    expect(
      sectionForSet(`## Approach\n\n## UI design\n\n{{design:${SET}}}\n`, SET),
    ).toBe('UI design');
  });

  it('accepts 12-char short design set ids', () => {
    const short = 'abc123xyz789';
    expect(splitPlanMarkers(`{{design:${short}}}`)).toEqual([
      { type: 'design', setId: short },
    ]);
    expect(stripDesignMarkers(`keep {{design:${short}}} text`)).toBe('keep  text');
  });
});
