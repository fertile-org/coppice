import '@testing-library/jest-dom/vitest';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DiffViewer } from './DiffViewer';

const patchState = vi.hoisted(() => ({ patch: '', base: '' }));

vi.mock('./useCodeReview', () => ({
  useFilePatch: () => ({
    data: { path: 'file', patch: patchState.patch },
    isLoading: false,
    isError: false,
    error: null,
  }),
  useFileContent: () => ({
    data: { path: 'file', side: 'base' as const, content: patchState.base },
    isLoading: false,
    isError: false,
  }),
}));

function patch(path: string, before: string, after: string) {
  return [
    `diff --git a/${path} b/${path}`,
    'index 1111111..2222222 100644',
    `--- a/${path}`,
    `+++ b/${path}`,
    '@@ -1 +1 @@',
    `-${before}`,
    `+${after}`,
  ].join('\n');
}

function renderDiff(
  path: string,
  onInlineCommentsChange: (comments: unknown) => void = () => {},
) {
  return render(
    <DiffViewer
      repoId="repo"
      worktreePath="/tmp/wt"
      baseBranch="main"
      filePath={path}
      viewType="split"
      ignoreWhitespace={false}
      inlineComments={[]}
      onInlineCommentsChange={onInlineCommentsChange}
    />,
  );
}

describe('DiffViewer tokens', () => {
  beforeEach(() => {
    patchState.patch = '';
    patchState.base = '';
  });

  it('expands hidden lines and comments on the file line number', () => {
    patchState.base = 'alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\n';
    patchState.patch = [
      'diff --git a/notes.txt b/notes.txt',
      'index 1111111..2222222 100644',
      '--- a/notes.txt',
      '+++ b/notes.txt',
      '@@ -4,3 +4,3 @@',
      ' delta',
      '-epsilon',
      '+EPSILON',
      ' zeta',
    ].join('\n');
    const onInlineCommentsChange = vi.fn();
    renderDiff('notes.txt', onInlineCommentsChange);

    expect(screen.queryByText('alpha')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Expand 3 lines above' }));
    const alpha = screen.getAllByText('alpha')[0];
    expect(alpha).toBeInTheDocument();

    fireEvent.click(alpha);
    fireEvent.change(screen.getByPlaceholderText('Leave a comment…'), {
      target: { value: 'check the context' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add comment' }));

    expect(onInlineCommentsChange).toHaveBeenCalledWith([
      expect.objectContaining({
        path: 'notes.txt',
        line: 1,
        body: 'check the context',
      }),
    ]);
  });

  it('marks only the edited word on a changed line', () => {
    patchState.patch = patch('src/app.ts', 'const value = "old";', 'const value = "new";');
    const { container } = renderDiff('src/app.ts');

    const inserted = container.querySelector('.diff-code-insert');
    const edit = inserted?.querySelector('.diff-code-edit');
    expect(edit).toBeTruthy();
    expect(edit).toHaveTextContent('new');
    expect(inserted).toHaveTextContent('const value = "new";');
    expect(edit?.textContent).not.toBe(inserted?.textContent);
    expect(inserted?.querySelector('.hljs-keyword')).toHaveTextContent('const');
  });

  it('colours rust and css tokens', () => {
    patchState.patch = patch('src/lib.rs', 'let value = 1;', 'let value = 2;');
    const rust = renderDiff('src/lib.rs');
    expect(rust.container.querySelector('.hljs-keyword')).toHaveTextContent('let');
    rust.unmount();

    patchState.patch = patch('src/app.css', 'color: red;', 'color: blue;');
    const css = renderDiff('src/app.css');
    expect(css.container.querySelector('[class*="hljs-"]')).toBeTruthy();
  });

  it('renders an unknown file type as plain text', () => {
    patchState.patch = patch('notes.txt', 'hello old', 'hello new');
    const { container } = renderDiff('notes.txt');
    const inserted = container.querySelector('.diff-code-insert');

    expect(inserted).toHaveTextContent('hello new');
    expect(container.querySelector('[class*="hljs-"]')).toBeNull();
  });

  it('opens a 100-hunk file', () => {
    const lines = [
      'diff --git a/src/app.ts b/src/app.ts',
      'index 1111111..2222222 100644',
      '--- a/src/app.ts',
      '+++ b/src/app.ts',
    ];
    for (let i = 0; i < 100; i += 1) {
      const line = i * 5 + 1;
      lines.push(`@@ -${line} +${line} @@`, `-const value${i} = "old";`, `+const value${i} = "new";`);
    }
    patchState.patch = lines.join('\n');
    const { container } = renderDiff('src/app.ts');
    expect(container.querySelectorAll('.diff-hunk')).toHaveLength(100);
    expect(container.querySelector('.diff-code-insert .diff-code-edit')).toHaveTextContent('new');
  });

  it('paints an edited token with the mark text colour', () => {
    const css = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), '../../index.css'),
      'utf8',
    );
    expect(css).toMatch(
      /\.code-review-diff \.diff-code-edit \[class\*="hljs-"\]\s*\{\s*color:\s*inherit;/,
    );
  });
});
