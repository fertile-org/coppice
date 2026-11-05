import '@testing-library/jest-dom/vitest';
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DiffViewer } from './DiffViewer';

const patchState = vi.hoisted(() => ({ patch: '' }));

vi.mock('./useCodeReview', () => ({
  useFilePatch: () => ({
    data: { path: 'file', patch: patchState.patch },
    isLoading: false,
    isError: false,
    error: null,
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

function renderDiff(path: string) {
  return render(
    <DiffViewer
      repoId="repo"
      worktreePath="/tmp/wt"
      baseBranch="main"
      filePath={path}
      viewType="split"
      inlineComments={[]}
      onInlineCommentsChange={() => {}}
    />,
  );
}

describe('DiffViewer tokens', () => {
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
});
