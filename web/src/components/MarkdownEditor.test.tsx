import '@testing-library/jest-dom/vitest';
import { createRef, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import {
  MarkdownEditor,
  type MarkdownEditorHandle,
} from './MarkdownEditor';

function renderEditor(value: string, onChange = vi.fn()) {
  const ref = createRef<MarkdownEditorHandle>();
  const utils = render(
    <MarkdownEditor
      ref={ref}
      value={value}
      onChange={onChange}
      aria-label="Body"
      placeholder="Write…"
    />,
  );
  return { ...utils, ref, onChange };
}

describe('MarkdownEditor', () => {
  it('renders initial markdown as rich text', () => {
    renderEditor('## Plan\n\nSome **bold** text');

    const textbox = screen.getByRole('textbox', { name: 'Body' });
    expect(textbox.querySelector('h2')).toHaveTextContent('Plan');
    expect(textbox.querySelector('strong')).toHaveTextContent('bold');
  });

  it.each([
    ['headings', '# Title\n\n## Section\n\n### Sub'],
    ['emphasis', 'Some **bold**, *italic*, ~~strike~~ and `code`.'],
    ['bullet list', '- one\n- two\n  - nested'],
    ['ordered list', '1. first\n2. second'],
    ['task list', '- [ ] todo\n- [x] done'],
    ['code block', '```rust\nfn main() {}\n```'],
    ['link', 'See [docs](https://example.com/docs).'],
    ['blockquote', '> quoted line'],
    ['mention text', 'Ping @pm-codex about the plan.'],
    ['table', '| a | b |\n| --- | --- |\n| 1 | 2 |'],
    ['identifiers', 'Rename snake_case_name in src/foo_bar.rs'],
    ['inline html text', 'Use <br> tags & <!-- note -->'],
    [
      'mixed document',
      '## Goal\n\nShip **it**.\n\n- [ ] write tests\n- [x] spec\n\n1. a\n2. b\n\n> note\n\n```ts\nconst x = 1;\n```\n\nDone: see [PR](https://x.dev/1).',
    ],
  ])('round-trips %s', (_name, markdown) => {
    const { ref } = renderEditor(markdown);
    expect(ref.current!.getMarkdown()).toBe(markdown);
  });

  it('toggles bold from the toolbar and emits markdown', () => {
    const { ref, onChange } = renderEditor('hello');

    act(() => {
      ref.current!.getEditor()!.commands.selectAll();
    });
    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(bold).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(bold);

    expect(onChange).toHaveBeenLastCalledWith('**hello**');
    expect(bold).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows the placeholder when empty', () => {
    renderEditor('');

    const textbox = screen.getByRole('textbox', { name: 'Body' });
    expect(textbox).toHaveAttribute('aria-placeholder', 'Write…');
    expect(textbox.querySelector('p[data-placeholder="Write…"]')).not.toBeNull();
  });

  it('does not emit changes for an untouched value', () => {
    const { onChange } = renderEditor('snake_case and *text*');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('updates when the value is reset externally', () => {
    function Harness() {
      const [value, setValue] = useState('draft **comment**');
      return (
        <>
          <MarkdownEditor value={value} onChange={setValue} aria-label="Body" />
          <button type="button" onClick={() => setValue('')}>
            Reset
          </button>
        </>
      );
    }
    render(<Harness />);
    const textbox = screen.getByRole('textbox', { name: 'Body' });
    expect(textbox).toHaveTextContent('draft comment');

    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));

    expect(textbox).toHaveTextContent('');
    expect(textbox.querySelector('strong')).toBeNull();
  });

  it('edits raw markdown in source mode', () => {
    const onChange = vi.fn();
    render(
      <MarkdownEditor value="**hi**" onChange={onChange} placeholder="Write…" />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Markdown source' }));
    const source = screen.getByPlaceholderText<HTMLTextAreaElement>('Write…');
    expect(source.value).toBe('**hi**');
    expect(screen.getByRole('button', { name: 'Bold' })).toBeDisabled();

    fireEvent.change(source, { target: { value: '# New' } });
    expect(onChange).toHaveBeenLastCalledWith('# New');
  });

  it('submits on Ctrl+Enter', () => {
    const onSubmit = vi.fn();
    render(
      <MarkdownEditor
        value="x"
        onChange={() => {}}
        onSubmit={onSubmit}
        aria-label="Body"
      />,
    );

    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Body' }), {
      key: 'Enter',
      ctrlKey: true,
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it('is read-only when disabled', () => {
    render(
      <MarkdownEditor value="x" onChange={() => {}} disabled aria-label="Body" />,
    );
    expect(screen.getByRole('textbox', { name: 'Body' })).toHaveAttribute(
      'contenteditable',
      'false',
    );
    expect(screen.getByRole('button', { name: 'Bold' })).toBeDisabled();
  });
});
