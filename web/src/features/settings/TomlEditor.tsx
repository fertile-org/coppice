import { StreamLanguage } from '@codemirror/language';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { StateEffect, StateField } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { basicSetup } from 'codemirror';
import { useEffect, useRef } from 'react';

const setErrorLine = StateEffect.define<number | null>();

const errorLineField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(decorations, transaction) {
    decorations = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setErrorLine)) {
        if (effect.value == null) {
          return Decoration.none;
        }
        const lineNo = Math.min(Math.max(effect.value, 1), transaction.state.doc.lines);
        const line = transaction.state.doc.line(lineNo);
        return Decoration.set([
          Decoration.line({ class: 'cm-error-line' }).range(line.from),
        ]);
      }
    }
    return decorations;
  },
  provide: (field) => EditorView.decorations.from(field),
});

const theme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--color-field-bg)',
    color: 'var(--color-text-primary)',
    fontSize: '13px',
    height: 'min(70vh, 36rem)',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.5',
  },
  '.cm-content': {
    padding: '12px 0',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--color-surface)',
    color: 'var(--color-text-muted)',
    borderRight: '1px solid var(--color-border)',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-error-line': {
    backgroundColor: 'var(--color-danger-muted)',
  },
});

interface TomlEditorProps {
  value: string;
  onChange: (value: string) => void;
  errorLine: number | null;
}

export function TomlEditor({ value, onChange, errorLine }: TomlEditorProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    const parent = parentRef.current;
    if (!parent) {
      return;
    }
    const view = new EditorView({
      parent,
      doc: value,
      extensions: [
        basicSetup,
        StreamLanguage.define(toml),
        errorLineField,
        theme,
        EditorView.contentAttributes.of({ 'aria-label': 'config.toml' }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
      ],
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // The editor owns the document after mount and syncs external updates below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    const line =
      errorLine != null && errorLine >= 1 && errorLine <= view.state.doc.lines
        ? errorLine
        : null;
    view.dispatch({ effects: setErrorLine.of(line) });
    if (line != null) {
      const pos = view.state.doc.line(line).from;
      view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: 'center' }) });
    }
  }, [errorLine, value]);

  return (
    <div
      ref={parentRef}
      data-testid="config-editor"
      className="overflow-hidden rounded-md border border-border"
    />
  );
}
