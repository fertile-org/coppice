import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChangedFilesPanel, VIRTUALIZE_FILES_AFTER } from './ChangedFilesPanel';

function files(count: number) {
  return Array.from({ length: count }, (_, i) => ({
    path: `src/file_${i}.ts`,
    status: 'modified',
    additions: 1,
    deletions: 0,
    notInPlan: false,
  }));
}

describe('ChangedFilesPanel', () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const height = this.hasAttribute('data-virtual-files') ? 800 : 52;
      return { x: 0, y: 0, top: 0, left: 0, bottom: height, right: 280, width: 280, height, toJSON() {} } as DOMRect;
    });
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-virtual-files') ? 800 : 52;
    });
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(280);
  });
  afterEach(() => vi.restoreAllMocks());

  it('lists every file when the diff is small', () => {
    render(<ChangedFilesPanel files={files(VIRTUALIZE_FILES_AFTER)} selectedPath={null} onSelect={() => {}} isLoading={false} />);
    expect(screen.getAllByRole('button')).toHaveLength(VIRTUALIZE_FILES_AFTER);
  });

  it('mounts only the rows in view for 3,000 changed files', () => {
    render(<ChangedFilesPanel files={files(3000)} selectedPath={null} onSelect={() => {}} isLoading={false} />);
    const rows = screen.getAllByRole('button').length;
    expect(rows).toBeGreaterThan(0);
    expect(rows).toBeLessThan(60);
    expect(screen.getByText('3000 files')).toBeInTheDocument();
  });
});
