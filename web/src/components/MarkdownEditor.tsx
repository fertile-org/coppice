import {
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type Ref,
} from 'react';
import { Extension, type Editor } from '@tiptap/core';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Text from '@tiptap/extension-text';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { TableKit } from '@tiptap/extension-table';
import { Placeholder } from '@tiptap/extensions';
import { Markdown, type MarkdownStorage } from 'tiptap-markdown';
import {
  Bold,
  Code,
  FileCode,
  Heading2,
  Heading3,
  Italic,
  Link as LinkIcon,
  List,
  ListChecks,
  ListOrdered,
  Quote,
  Redo2,
  SquareCode,
  Strikethrough,
  Undo2,
} from 'lucide-react';
import { cn } from '../lib/utils';

export interface MarkdownEditorHandle {
  focus(): void;
  /** Replace `length` characters before the cursor with `text` (e.g. completing an @mention). */
  replaceTextBeforeCursor(length: number, text: string): void;
  /** Markdown serialized from the current rich-text document. */
  getMarkdown(): string;
  getEditor(): Editor | null;
}

export interface MarkdownEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  placeholder?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  minHeight?: number | string;
  onSubmit?: () => void;
  /** Text of the current block up to the cursor; fires on edits and selection moves. */
  onTextBeforeCursorChange?: (text: string) => void;
  'aria-label'?: string;
  id?: string;
  className?: string;
  ref?: Ref<MarkdownEditorHandle>;
}

type Mode = 'rich' | 'source';

function getMarkdown(editor: Editor): string {
  const storage = (editor.storage as unknown as { markdown: MarkdownStorage })
    .markdown;
  return storage.getMarkdown().replace(/\n+$/, '');
}

// With `html: false` raw HTML stays literal text (e.g. `<!-- coppice-agent-requests: … -->` markers);
// tiptap-markdown's default text serializer would entity-escape it.
const LiteralText = Text.extend({
  addStorage() {
    return {
      markdown: {
        serialize(state: { text(text: string): void }, node: { text: string }) {
          state.text(node.text);
        },
      },
    };
  },
});

// tiptap-markdown only tracks list tightness on bullet/ordered lists; without this, task lists serialize loose.
const TaskListTightness = Extension.create({
  name: 'coppiceTaskListTightness',
  addGlobalAttributes() {
    return [
      {
        types: ['taskList'],
        attributes: {
          tight: {
            default: true,
            parseHTML: (element) =>
              element.getAttribute('data-tight') === 'true' ||
              !element.querySelector('p'),
            renderHTML: (attributes) => ({
              'data-tight': attributes.tight ? 'true' : null,
            }),
          },
        },
      },
    ];
  },
});

function textBeforeCursor(editor: Editor): string {
  const { $from } = editor.state.selection;
  return $from.parent.textBetween(0, $from.parentOffset, undefined, '\ufffc');
}

function editorAttributes({
  ariaLabel,
  id,
  placeholder,
}: {
  ariaLabel?: string;
  id?: string;
  placeholder?: string;
}): Record<string, string> {
  const attrs: Record<string, string> = {
    class: 'markdown-editor-content',
    role: 'textbox',
    'aria-multiline': 'true',
  };
  if (ariaLabel) attrs['aria-label'] = ariaLabel;
  if (id) attrs.id = id;
  if (placeholder) attrs['aria-placeholder'] = placeholder;
  return attrs;
}

function ToolbarButton({
  label,
  shortcut,
  active = false,
  disabled,
  onClick,
  children,
}: {
  label: string;
  shortcut?: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={shortcut ? `${label} (${shortcut})` : label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className={cn(
        'inline-flex size-7 items-center justify-center rounded text-text-secondary transition-colors duration-fast hover:bg-paper-200 hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:pointer-events-none disabled:opacity-40',
        active && 'bg-accent-muted text-accent hover:bg-accent-muted hover:text-accent',
      )}
    >
      {children}
    </button>
  );
}

function Separator() {
  return <span aria-hidden="true" className="mx-0.5 h-4 w-px bg-border" />;
}

const isMac =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
const MOD = isMac ? '⌘' : 'Ctrl';

export function MarkdownEditor({
  value,
  onChange,
  placeholder,
  disabled = false,
  autoFocus = false,
  minHeight = 120,
  onSubmit,
  onTextBeforeCursorChange,
  'aria-label': ariaLabel,
  id,
  className,
  ref,
}: MarkdownEditorProps) {
  const [mode, setMode] = useState<Mode>('rich');
  const [linkDraft, setLinkDraft] = useState<string | null>(null);
  const sourceRef = useRef<HTMLTextAreaElement>(null);
  const linkInputRef = useRef<HTMLInputElement>(null);
  // Last markdown we emitted (or loaded); external `value` changes that differ get pushed into the editor.
  const lastValueRef = useRef(value);

  const callbacks = useRef({
    onChange,
    onSubmit,
    onTextBeforeCursorChange,
    openLink: () => {},
  });

  const editor = useEditor({
    content: value,
    editable: !disabled,
    autofocus: autoFocus ? 'end' : false,
    extensions: [
      StarterKit.configure({
        underline: false,
        text: false,
        link: {
          openOnClick: false,
          autolink: true,
          defaultProtocol: 'https',
        },
      }),
      LiteralText,
      TaskList,
      TaskItem.configure({ nested: true }),
      TaskListTightness,
      TableKit.configure({ table: { resizable: false } }),
      Placeholder.configure({
        placeholder: ({ editor: e }) => {
          const attrs = e.options.editorProps.attributes;
          return (typeof attrs === 'object' && attrs?.['aria-placeholder']) || '';
        },
      }),
      Markdown.configure({
        html: false,
        tightLists: true,
        bulletListMarker: '-',
        linkify: false,
        breaks: false,
        transformPastedText: true,
        transformCopiedText: false,
      }),
      Extension.create({
        name: 'coppiceMarkdownEditorKeys',
        addKeyboardShortcuts() {
          return {
            'Mod-Enter': () => {
              if (!callbacks.current.onSubmit) return false;
              callbacks.current.onSubmit();
              return true;
            },
            'Mod-k': () => {
              callbacks.current.openLink();
              return true;
            },
            'Mod-Shift-x': ({ editor: e }) => e.commands.toggleStrike(),
          };
        },
      }),
    ],
    editorProps: {
      attributes: editorAttributes({ ariaLabel, id, placeholder }),
    },
    onUpdate: ({ editor: e }) => {
      const markdown = getMarkdown(e);
      lastValueRef.current = markdown;
      callbacks.current.onChange(markdown);
      callbacks.current.onTextBeforeCursorChange?.(textBeforeCursor(e));
    },
    onSelectionUpdate: ({ editor: e }) => {
      callbacks.current.onTextBeforeCursorChange?.(textBeforeCursor(e));
    },
  });

  useEffect(() => {
    if (!editor || value === lastValueRef.current) return;
    lastValueRef.current = value;
    editor.commands.setContent(value, { emitUpdate: false });
  }, [editor, value]);

  useEffect(() => {
    editor?.setEditable(!disabled, false);
  }, [editor, disabled]);

  useEffect(() => {
    if (!editor) return;
    editor.setOptions({
      editorProps: {
        attributes: editorAttributes({
          ariaLabel,
          id: mode === 'rich' ? id : undefined,
          placeholder,
        }),
      },
    });
  }, [editor, ariaLabel, id, mode, placeholder]);

  const active = useEditorState({
    editor,
    selector: ({ editor: e }) =>
      e
        ? {
            bold: e.isActive('bold'),
            italic: e.isActive('italic'),
            strike: e.isActive('strike'),
            code: e.isActive('code'),
            h2: e.isActive('heading', { level: 2 }),
            h3: e.isActive('heading', { level: 3 }),
            bulletList: e.isActive('bulletList'),
            orderedList: e.isActive('orderedList'),
            taskList: e.isActive('taskList'),
            blockquote: e.isActive('blockquote'),
            codeBlock: e.isActive('codeBlock'),
            link: e.isActive('link'),
            canUndo: e.can().undo(),
            canRedo: e.can().redo(),
          }
        : null,
  });

  function openLink() {
    if (!editor || disabled) return;
    const href = (editor.getAttributes('link').href as string | undefined) ?? '';
    setLinkDraft(href);
    requestAnimationFrame(() => linkInputRef.current?.select());
  }

  useLayoutEffect(() => {
    callbacks.current = {
      onChange,
      onSubmit,
      onTextBeforeCursorChange,
      openLink,
    };
  });

  function applyLink() {
    if (!editor || linkDraft === null) return;
    const href = linkDraft.trim();
    const chain = editor.chain().focus().extendMarkRange('link');
    if (href) {
      if (editor.state.selection.empty && !editor.isActive('link')) {
        chain
          .insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] })
          .run();
      } else {
        chain.setLink({ href }).run();
      }
    } else {
      chain.unsetLink().run();
    }
    setLinkDraft(null);
  }

  function emitSource(next: string) {
    lastValueRef.current = next;
    callbacks.current.onChange(next);
  }

  function syncSourceCursor(el: HTMLTextAreaElement) {
    const before = el.value.slice(0, el.selectionStart);
    onTextBeforeCursorChange?.(before.slice(before.lastIndexOf('\n') + 1));
  }

  function toggleMode() {
    if (mode === 'source') {
      if (editor) {
        lastValueRef.current = value;
        editor.commands.setContent(value, { emitUpdate: false });
      }
      setMode('rich');
      requestAnimationFrame(() => editor?.commands.focus('end'));
    } else {
      setLinkDraft(null);
      setMode('source');
      requestAnimationFrame(() => sourceRef.current?.focus());
    }
  }

  useImperativeHandle(
    ref,
    () => ({
      focus() {
        if (mode === 'source') sourceRef.current?.focus();
        else editor?.commands.focus();
      },
      replaceTextBeforeCursor(length, text) {
        if (mode === 'source') {
          const el = sourceRef.current;
          if (!el) return;
          const cursor = el.selectionStart;
          const start = Math.max(0, cursor - length);
          const next = el.value.slice(0, start) + text + el.value.slice(cursor);
          lastValueRef.current = next;
          callbacks.current.onChange(next);
          const nextCursor = start + text.length;
          requestAnimationFrame(() => {
            el.focus();
            el.setSelectionRange(nextCursor, nextCursor);
          });
          return;
        }
        if (!editor) return;
        const { from } = editor.state.selection;
        editor
          .chain()
          .focus()
          .deleteRange({ from: Math.max(0, from - length), to: from })
          .insertContent(text)
          .run();
      },
      getMarkdown: () => (editor ? getMarkdown(editor) : lastValueRef.current),
      getEditor: () => editor,
    }),
    [editor, mode],
  );

  const toolsDisabled = disabled || mode === 'source' || !editor;
  const minHeightCss = typeof minHeight === 'number' ? `${minHeight}px` : minHeight;

  return (
    <div
      className={cn(
        'markdown-editor field-control flex w-full flex-col overflow-hidden',
        disabled && 'markdown-editor--disabled',
        className,
      )}
    >
      <div
        role="toolbar"
        aria-label="Formatting"
        className="flex flex-wrap items-center gap-0.5 border-b border-border px-1.5 py-1"
      >
        <ToolbarButton label="Bold" shortcut={`${MOD}+B`} active={active?.bold} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleBold().run()}>
          <Bold className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Italic" shortcut={`${MOD}+I`} active={active?.italic} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleItalic().run()}>
          <Italic className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Strikethrough" shortcut={`${MOD}+Shift+X`} active={active?.strike} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleStrike().run()}>
          <Strikethrough className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Inline code" shortcut={`${MOD}+E`} active={active?.code} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleCode().run()}>
          <Code className="size-4" />
        </ToolbarButton>
        <Separator />
        <ToolbarButton label="Heading 2" shortcut={`${MOD}+Alt+2`} active={active?.h2} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}>
          <Heading2 className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Heading 3" shortcut={`${MOD}+Alt+3`} active={active?.h3} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()}>
          <Heading3 className="size-4" />
        </ToolbarButton>
        <Separator />
        <ToolbarButton label="Bullet list" shortcut={`${MOD}+Shift+8`} active={active?.bulletList} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleBulletList().run()}>
          <List className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Ordered list" shortcut={`${MOD}+Shift+7`} active={active?.orderedList} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleOrderedList().run()}>
          <ListOrdered className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Task list" shortcut={`${MOD}+Shift+9`} active={active?.taskList} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleTaskList().run()}>
          <ListChecks className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Blockquote" shortcut={`${MOD}+Shift+B`} active={active?.blockquote} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleBlockquote().run()}>
          <Quote className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Code block" shortcut={`${MOD}+Alt+C`} active={active?.codeBlock} disabled={toolsDisabled} onClick={() => editor?.chain().focus().toggleCodeBlock().run()}>
          <SquareCode className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Link" shortcut={`${MOD}+K`} active={active?.link} disabled={toolsDisabled} onClick={openLink}>
          <LinkIcon className="size-4" />
        </ToolbarButton>
        <Separator />
        <ToolbarButton label="Undo" shortcut={`${MOD}+Z`} disabled={toolsDisabled || !active?.canUndo} onClick={() => editor?.chain().focus().undo().run()}>
          <Undo2 className="size-4" />
        </ToolbarButton>
        <ToolbarButton label="Redo" shortcut={`${MOD}+Shift+Z`} disabled={toolsDisabled || !active?.canRedo} onClick={() => editor?.chain().focus().redo().run()}>
          <Redo2 className="size-4" />
        </ToolbarButton>
        <span className="ml-auto" />
        <ToolbarButton
          label={mode === 'source' ? 'Rich text editor' : 'Markdown source'}
          active={mode === 'source'}
          disabled={disabled}
          onClick={toggleMode}
        >
          <FileCode className="size-4" />
        </ToolbarButton>
      </div>

      {linkDraft !== null && mode === 'rich' && (
        <div className="flex items-center gap-2 border-b border-border px-2 py-1.5">
          <LinkIcon className="size-3.5 shrink-0 text-text-muted" aria-hidden="true" />
          <input
            ref={linkInputRef}
            type="url"
            aria-label="Link URL"
            placeholder="https://… (empty removes link)"
            value={linkDraft}
            onChange={(e) => setLinkDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                applyLink();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                setLinkDraft(null);
                editor?.commands.focus();
              }
            }}
            className="min-w-0 flex-1 bg-transparent font-body text-sm text-text-primary outline-none placeholder:text-text-muted"
          />
          <button
            type="button"
            onClick={applyLink}
            className="rounded px-2 py-0.5 font-body text-xs font-medium text-accent hover:bg-accent-muted"
          >
            Apply
          </button>
        </div>
      )}

      <EditorContent
        editor={editor}
        hidden={mode === 'source'}
        className="markdown-editor-body overflow-y-auto"
        style={{ minHeight: minHeightCss }}
      />

      {mode === 'source' && (
        <textarea
          ref={sourceRef}
          id={id}
          aria-label={ariaLabel}
          value={value}
          placeholder={placeholder}
          disabled={disabled}
          onChange={(e) => {
            emitSource(e.target.value);
            syncSourceCursor(e.target);
          }}
          onSelect={(e) => syncSourceCursor(e.currentTarget)}
          onKeyDown={(e) => {
            if (onSubmit && e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              onSubmit();
            }
          }}
          style={{ minHeight: minHeightCss }}
          className="w-full resize-y bg-transparent px-3 py-2 font-mono text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-muted"
        />
      )}
    </div>
  );
}
