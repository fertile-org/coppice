import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChangedFilesPanel } from './ChangedFilesPanel';

describe('ChangedFilesPanel', () => {
  it('tags only files the plan did not name', () => {
    render(
      <ChangedFilesPanel
        files={[
          { path: 'src/planned.rs', status: 'modified', additions: 1, deletions: 0 },
          {
            path: 'src/extra.rs',
            status: 'added',
            additions: 2,
            deletions: 0,
            notInPlan: true,
          },
        ]}
        selectedPath="src/extra.rs"
        onSelect={() => {}}
      />,
    );

    expect(screen.getAllByText('Not in the plan')).toHaveLength(1);
    expect(screen.getByText('planned.rs')).toBeVisible();
    expect(screen.getByText('extra.rs')).toBeVisible();
  });

  it('keeps the filename when the folder is long, and dims a viewed file', () => {
    const path = 'web/src/features/settings/appearance/ThemeToggle.tsx';
    render(
      <ChangedFilesPanel
        files={[{ path, status: 'added', additions: 48, deletions: 0 }]}
        selectedPath={path}
        onSelect={() => {}}
        viewedPaths={new Set([path])}
      />,
    );

    const row = screen.getByRole('button', { name: /ThemeToggle\.tsx/ });
    expect(row).toHaveAttribute('title', path);
    expect(row.querySelector('.line-through')).toHaveTextContent('ThemeToggle.tsx');
    expect(row).toHaveTextContent('web/src/features/settings/appearance/');
    expect(row.querySelector('[class*="direction:rtl"]')).toHaveTextContent(
      'web/src/features/settings/appearance/',
    );
    expect(screen.getByText('✓')).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '1');
    expect(screen.getByText('1 of 1 viewed')).toBeVisible();
  });
});