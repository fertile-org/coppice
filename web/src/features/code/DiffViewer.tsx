import {
  useCallback,
  useMemo,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';
import {
  Decoration,
  Diff,
  Hunk,
  expandFromRawCode,
  getChangeKey,
  getCollapsedLinesCountBetween,
  isDelete,
  isInsert,
  parseDiff,
  type ChangeData,
  type ChangeEventArgs,
  type HunkData,
} from 'react-diff-view';
import { Button } from '../../components/ui/button';
import { Textarea } from '../../components/ui/textarea';
import type { InlineComment } from '../../lib/schemas/codeReview';
import { diffTokens } from './diffHighlight';
import { useFileContent, useFilePatch } from './useCodeReview';
import 'react-diff-view/style/index.css';

export interface InlineCommentDraft extends InlineComment {
  changeKey: string;
}

interface DiffViewerProps {
  repoId: string;
  worktreePath: string;
  baseBranch: string;
  filePath: string | undefined;
  viewType: 'split' | 'unified';
  ignoreWhitespace: boolean;
  inlineComments: InlineCommentDraft[];
  onInlineCommentsChange: (comments: InlineCommentDraft[]) => void;
}

function changeToSide(
  change: ChangeData,
  eventSide?: 'old' | 'new',
): InlineComment['side'] {
  if (isInsert(change)) return 'new';
  if (isDelete(change)) return 'delete';
  return eventSide ?? 'new';
}

function changeToLineNumber(
  change: ChangeData,
  side: InlineComment['side'],
): number {
  if (isInsert(change)) return change.lineNumber;
  if (isDelete(change)) return change.lineNumber;
  if (side === 'old') return change.oldLineNumber;
  return change.newLineNumber;
}

function InlineCommentWidget({
  body,
  onSave,
  onCancel,
  onRemove,
  isNew,
}: {
  body: string;
  onSave: (body: string) => void;
  onCancel: () => void;
  onRemove?: () => void;
  isNew: boolean;
}) {
  const [draft, setDraft] = useState(body);

  return (
    <div className="rounded-md border border-border bg-paper-50 p-3 shadow-sm">
      <Textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Leave a comment…"
        rows={3}
        className="min-h-[72px] bg-field-bg"
        autoFocus={isNew}
      />
      <div className="mt-2 flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={!draft.trim()}
          onClick={() => onSave(draft.trim())}
        >
          {isNew ? 'Add comment' : 'Save'}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        {!isNew && onRemove && (
          <Button
            type="button"
            variant="destructive"
            size="sm"
            className="ml-auto"
            onClick={onRemove}
          >
            Remove
          </Button>
        )}
      </div>
    </div>
  );
}

function PendingCommentBubble({
  body,
  onEdit,
  onRemove,
}: {
  body: string;
  onEdit: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="rounded-md border border-accent-muted bg-accent-muted/30 p-3">
      <p className="whitespace-pre-wrap font-body text-sm text-text-primary">
        {body}
      </p>
      <div className="mt-2 flex gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onEdit}>
          Edit
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onRemove}>
          Remove
        </Button>
      </div>
    </div>
  );
}

/** Hunks mounted at once for one file. */
export const HUNK_WINDOW = 100;

function sourceLineCount(source: string): number {
  if (source.length === 0) return 0;
  const newlineCount = source.split('\n').length - 1;
  return source.endsWith('\n') ? newlineCount : newlineCount + 1;
}

function ExpandRow({
  count,
  where,
  onClick,
}: {
  count: number;
  where: 'above' | 'between' | 'below';
  onClick: () => void;
}) {
  const lines = count === 1 ? 'line' : 'lines';
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full px-3 py-1 text-left font-body text-xs text-text-secondary hover:text-text-primary"
    >
      Expand {count} {lines} {where}
    </button>
  );
}

function hunkRows(
  hunks: HunkData[],
  oldSource: string | undefined,
  showBelow: boolean,
  onExpand: (start: number, end: number) => void,
): ReactElement[] {
  const rows: ReactElement[] = [];
  if (hunks.length === 0) return rows;

  const pushExpand = (
    key: string,
    count: number,
    where: 'above' | 'between' | 'below',
    start: number,
    end: number,
  ) => {
    if (!oldSource || count <= 0) return;
    rows.push(
      <Decoration key={key}>
        <ExpandRow count={count} where={where} onClick={() => onExpand(start, end)} />
      </Decoration>,
    );
  };

  const first = hunks[0];
  pushExpand('above', first.oldStart - 1, 'above', 1, first.oldStart);

  hunks.forEach((hunk, index) => {
    rows.push(<Hunk key={`${hunk.oldStart}-${hunk.newStart}-${index}`} hunk={hunk} />);
    const next = hunks[index + 1];
    if (!next) return;
    const start = hunk.oldStart + hunk.oldLines;
    pushExpand(
      `between-${start}`,
      getCollapsedLinesCountBetween(hunk, next),
      'between',
      start,
      next.oldStart,
    );
  });

  if (showBelow) {
    const last = hunks[hunks.length - 1];
    const start = last.oldStart + last.oldLines;
    const below = sourceLineCount(oldSource ?? '') - (start - 1);
    pushExpand('below', below, 'below', start, start + Math.max(below, 0));
  }

  return rows;
}

export function DiffViewer({
  repoId,
  worktreePath,
  baseBranch,
  filePath,
  viewType,
  ignoreWhitespace,
  inlineComments,
  onInlineCommentsChange,
}: DiffViewerProps) {
  // Hunks mounted for the current file; more on request.
  const [hunkWindow, setHunkWindow] = useState({ path: filePath, count: HUNK_WINDOW });
  const shownHunks = hunkWindow.path === filePath ? hunkWindow.count : HUNK_WINDOW;
  const setShownHunks = (next: (count: number) => number) =>
    setHunkWindow({ path: filePath, count: next(shownHunks) });
  const [editingChangeKey, setEditingChangeKey] = useState<string | null>(null);
  const [composingChangeKey, setComposingChangeKey] = useState<string | null>(
    null,
  );
  const [composingSide, setComposingSide] = useState<'old' | 'new' | undefined>(
    undefined,
  );

  const { data, isLoading, isError, error } = useFilePatch(
    repoId,
    worktreePath,
    baseBranch,
    filePath,
    ignoreWhitespace,
  );
  const baseContent = useFileContent(
    repoId,
    worktreePath,
    baseBranch,
    filePath,
    'base',
  );

  const parsedFile = useMemo(() => {
    if (!data?.patch?.trim()) return null;
    const files = parseDiff(data.patch);
    return files[0] ?? null;
  }, [data?.patch]);

  const expansionKey = `${filePath ?? ''}\0${data?.patch ?? ''}`;
  const [expansion, setExpansion] = useState<{
    key: string;
    ranges: Array<[number, number]>;
  }>({ key: '', ranges: [] });
  const ranges = expansion.key === expansionKey ? expansion.ranges : [];
  const expandRange = useCallback(
    (start: number, end: number) => {
      setExpansion((current) => {
        const currentRanges = current.key === expansionKey ? current.ranges : [];
        return { key: expansionKey, ranges: [...currentRanges, [start, end]] };
      });
    },
    [expansionKey],
  );

  const oldSource = baseContent.data?.content;
  const renderingHunks = useMemo(() => {
    const hunks = parsedFile?.hunks ?? [];
    if (!oldSource || ranges.length === 0) return hunks;
    return ranges.reduce(
      (current, [start, end]) => expandFromRawCode(current, oldSource, start, end),
      hunks,
    );
  }, [oldSource, parsedFile, ranges]);

  const fileComments = useMemo(
    () => inlineComments.filter((c) => c.path === filePath),
    [inlineComments, filePath],
  );

  const commentsByChangeKey = useMemo(() => {
    const map = new Map<string, InlineCommentDraft>();
    for (const comment of fileComments) {
      map.set(comment.changeKey, comment);
    }
    return map;
  }, [fileComments]);

  const handleLineClick = useCallback(
    ({ change, side }: ChangeEventArgs) => {
      if (!change || !filePath) return;
      const changeKey = getChangeKey(change);
      if (commentsByChangeKey.has(changeKey)) {
        setEditingChangeKey(changeKey);
        setComposingChangeKey(null);
        setComposingSide(undefined);
        return;
      }
      setComposingChangeKey(changeKey);
      setComposingSide(side);
      setEditingChangeKey(null);
    },
    [commentsByChangeKey, filePath],
  );

  const upsertComment = useCallback(
    (
      changeKey: string,
      change: ChangeData,
      eventSide: InlineComment['side'] | undefined,
      body: string,
    ) => {
      if (!filePath) return;
      const side = eventSide ?? changeToSide(change);
      const line = changeToLineNumber(change, side);
      const next: InlineCommentDraft = {
        changeKey,
        path: filePath,
        line,
        side,
        body,
      };
      const others = inlineComments.filter((c) => c.changeKey !== changeKey);
      onInlineCommentsChange([...others, next]);
      setComposingChangeKey(null);
      setEditingChangeKey(null);
    },
    [filePath, inlineComments, onInlineCommentsChange],
  );

  const removeComment = useCallback(
    (changeKey: string) => {
      onInlineCommentsChange(
        inlineComments.filter((c) => c.changeKey !== changeKey),
      );
      setComposingChangeKey(null);
      setEditingChangeKey(null);
    },
    [inlineComments, onInlineCommentsChange],
  );

  // Built once per file, so compose and edit state changes don't walk every line.
  const changesByKey = useMemo(() => {
    const map = new Map<string, ChangeData>();
    for (const hunk of renderingHunks) {
      for (const change of hunk.changes) {
        map.set(getChangeKey(change), change);
      }
    }
    return map;
  }, [renderingHunks]);

  const widgets = useMemo(() => {
    if (renderingHunks.length === 0 || !filePath) return {};

    const result: Record<string, ReactNode> = {};
    // Only lines with a comment or an open composer get a widget.
    const keys = new Set(commentsByChangeKey.keys());
    if (composingChangeKey) keys.add(composingChangeKey);

    for (const changeKey of keys) {
      const change = changesByKey.get(changeKey);
      if (!change) continue;
      const existing = commentsByChangeKey.get(changeKey);

      if (composingChangeKey === changeKey) {
        result[changeKey] = (
          <InlineCommentWidget
            key={`compose-${changeKey}`}
            body=""
            isNew
            onCancel={() => {
              setComposingChangeKey(null);
              setComposingSide(undefined);
            }}
            onSave={(body) =>
              upsertComment(
                changeKey,
                change,
                changeToSide(change, composingSide),
                body,
              )
            }
          />
        );
        continue;
      }

      if (editingChangeKey === changeKey && existing) {
        result[changeKey] = (
          <InlineCommentWidget
            key={`edit-${changeKey}`}
            body={existing.body}
            isNew={false}
            onCancel={() => setEditingChangeKey(null)}
            onRemove={() => removeComment(changeKey)}
            onSave={(body) =>
              upsertComment(changeKey, change, existing.side, body)
            }
          />
        );
        continue;
      }

      if (existing) {
        result[changeKey] = (
          <PendingCommentBubble
            key={`pending-${changeKey}`}
            body={existing.body}
            onEdit={() => {
              setEditingChangeKey(changeKey);
              setComposingChangeKey(null);
            }}
            onRemove={() => removeComment(changeKey)}
          />
        );
      }
    }

    return result;
  }, [
    changesByKey,
    commentsByChangeKey,
    composingChangeKey,
    composingSide,
    editingChangeKey,
    filePath,
    removeComment,
    renderingHunks,
    upsertComment,
  ]);

  const lineEvents = useMemo(
    () => ({
      onClick: handleLineClick,
    }),
    [handleLineClick],
  );

  const visibleHunks = renderingHunks.slice(0, shownHunks);
  const tokens = useMemo(() => {
    const visible = renderingHunks.slice(0, shownHunks);
    if (visible.length === 0) return null;
    return diffTokens(visible, filePath);
  }, [filePath, renderingHunks, shownHunks]);

  if (!filePath) {
    return (
      <div className="flex h-full items-center justify-center bg-background p-8">
        <p className="font-body text-sm text-text-secondary">
          Select a file to view its diff.
        </p>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center bg-background p-8">
        <p className="font-body text-sm text-text-secondary">Loading diff…</p>
      </div>
    );
  }

  if (isError) {
    const message =
      error instanceof Error && error.message.includes('413')
        ? 'File too large to diff inline.'
        : 'Unable to load diff for this file.';
    return (
      <div className="flex h-full items-center justify-center bg-background p-8">
        <p className="font-body text-sm text-text-secondary">{message}</p>
      </div>
    );
  }

  if (!parsedFile || parsedFile.hunks.length === 0) {
    return (
      <div className="flex h-full items-center justify-center bg-background p-8">
        <p className="font-body text-sm text-text-secondary">
          No diff content for this file.
        </p>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-auto bg-background p-4">
      <div className="code-review-diff overflow-x-auto rounded-lg border border-border bg-paper-50">
        <Diff
          viewType={viewType}
          diffType={parsedFile.type}
          hunks={visibleHunks}
          tokens={tokens}
          widgets={widgets}
          gutterType="default"
          gutterEvents={lineEvents}
          codeEvents={lineEvents}
          optimizeSelection={false}
          className="!text-sm"
        >
          {(hunks) =>
            hunkRows(
              hunks,
              oldSource,
              hunks.length === renderingHunks.length,
              expandRange,
            )
          }
        </Diff>
      </div>
      {renderingHunks.length > shownHunks && (
        <button
          type="button"
          onClick={() => setShownHunks((count) => count + HUNK_WINDOW)}
          className="mt-3 font-body text-xs text-text-secondary underline"
        >
          Show more changes ({renderingHunks.length - shownHunks} hunks)
        </button>
      )}
      <p className="mt-3 font-body text-xs text-text-muted">
        Click a line number to add an inline comment.
      </p>
    </div>
  );
}
