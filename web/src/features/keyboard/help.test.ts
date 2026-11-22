import { describe, expect, it } from 'vitest';
import { helpSections } from './help';
import { omittedActionIds } from './registry';

describe('shortcut help rows', () => {
  it('groups shortcuts and omits admin navigation for members', () => {
    const sections = helpSections({ omitted: new Set(), platform: 'linux' });
    expect(sections.map((section) => section.group)).toEqual(['Navigate', 'Tickets', 'Review']);
    expect(sections[0]?.rows.filter((row) => row.id.startsWith('board.column.'))).toHaveLength(9);

    const member = helpSections({
      omitted: omittedActionIds({ isAdmin: false, desktopMode: false }),
      platform: 'linux',
    });
    const macos = helpSections({ omitted: new Set(), platform: 'macos' });
    const accept = macos
      .flatMap((section) => section.rows)
      .find((row) => row.id === 'review.accept');
    expect(accept?.shortcut).toBe('⌘+Shift+Enter');

    const labels = member.flatMap((section) => section.rows.map((row) => row.label));
    expect(labels).toContain('Go to Boards');
    expect(labels).not.toContain('Go to Plugins');
    expect(labels).not.toContain('Go to Users');
    expect(labels).not.toContain('Go to System');
    expect(labels).not.toContain('Go to Settings');

    const desktop = helpSections({
      omitted: omittedActionIds({ isAdmin: true, desktopMode: true }),
      platform: 'linux',
    });
    const desktopLabels = desktop.flatMap((section) =>
      section.rows.map((row) => row.label),
    );
    expect(desktopLabels).not.toContain('Go to Users');
    expect(desktopLabels).toContain('Go to Settings');
  });
});
