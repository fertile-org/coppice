import { describe, expect, it } from 'vitest';
import { helpSections } from './help';
import { omittedActionIds } from './registry';

describe('shortcut help rows', () => {
  it('keeps Go to column 1–9 and groups the keymap into Navigate, Tickets, and Review', () => {
    const sections = helpSections({ omitted: new Set(), platform: 'linux' });

    expect(sections.map((section) => section.group)).toEqual([
      'Navigate',
      'Tickets',
      'Review',
    ]);

    const navigate = sections[0]?.rows ?? [];
    const columnRows = navigate.filter((row) => row.id.startsWith('board.column.'));
    expect(columnRows.map((row) => row.label)).toEqual([
      'Go to column 1',
      'Go to column 2',
      'Go to column 3',
      'Go to column 4',
      'Go to column 5',
      'Go to column 6',
      'Go to column 7',
      'Go to column 8',
      'Go to column 9',
    ]);
    expect(columnRows.map((row) => row.shortcut)).toEqual([
      'G then 1',
      'G then 2',
      'G then 3',
      'G then 4',
      'G then 5',
      'G then 6',
      'G then 7',
      'G then 8',
      'G then 9',
    ]);
    expect(navigate.map((row) => row.label)).not.toContain('Go to Backlog column');
    expect(navigate.map((row) => row.label)).toContain('Show keyboard shortcuts');
    expect(sections[1]?.rows.map((row) => row.label)).toEqual([
      'Open ticket',
      'Create ticket',
      'Assign ticket',
      'Move to next column',
      'Move to previous column',
      'Post comment',
    ]);
    expect(sections[2]?.rows.map((row) => [row.label, row.shortcut])).toEqual([
      ['Approve plan', 'R then P'],
      ['Accept…', 'Ctrl+Shift+Enter'],
      ['Merge…', 'R then M'],
      ['Open code review', 'D'],
    ]);
  });

  it('draws Mod as ⌘ on macOS and omits admin Go to rows for members', () => {
    const macos = helpSections({ omitted: new Set(), platform: 'macos' });
    const accept = macos
      .flatMap((section) => section.rows)
      .find((row) => row.id === 'review.accept');
    expect(accept?.shortcut).toBe('⌘+Shift+Enter');

    const member = helpSections({
      omitted: omittedActionIds({ isAdmin: false, desktopMode: false }),
      platform: 'linux',
    });
    const labels = member.flatMap((section) => section.rows.map((row) => row.label));
    expect(labels).toContain('Go to Boards');
    expect(labels).not.toContain('Go to Plugins');
    expect(labels).not.toContain('Go to Users');
    expect(labels).not.toContain('Go to Tools');
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
