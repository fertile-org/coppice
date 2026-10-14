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
    expect(screen.getByText('src/planned.rs')).toBeVisible();
    expect(screen.getByText('src/extra.rs')).toBeVisible();
  });
});