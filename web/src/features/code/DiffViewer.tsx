import {
  useCallback,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  Diff,
  Hunk,
  getChangeKey,
  isDelete,
  isInsert,
  parseDiff,
  type ChangeData,
  type ChangeEventArgs,
} from 'react-diff-view';
import { Button } from '../../components/ui/button';
import { Textarea } from '../../components/ui/textarea';
import type { InlineComment } from '../../lib/schemas/codeReview';
import { useFilePatch } from './useCodeReview';
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
        className="min-h-[72px] bg-white"
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

export function DiffViewer({
  repoId,
  worktreePath,
  baseBranch,
  filePath,
  viewType,
  inlineComments,
  onInlineCommentsChange,
}: DiffViewerProps) {
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
  );

  const parsedFile = useMemo(() => {
    if (!data?.patch?.trim()) return null;
    const files = parseDiff(data.patch);
    return files[0] ?? null;
  }, [data?.patch]);

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

  const widgets = useMemo(() => {
    if (!parsedFile || !filePath) return {};

    const result: Record<string, ReactNode> = {};
    const allChanges = parsedFile.hunks.flatMap((hunk) => hunk.changes);

    for (const change of allChanges) {
      const changeKey = getChangeKey(change);
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
    commentsByChangeKey,
    composingChangeKey,
    composingSide,
    editingChangeKey,
    filePath,
    parsedFile,
    removeComment,
    upsertComment,
  ]);

  const lineEvents = useMemo(
    () => ({
      onClick: handleLineClick,
    }),
    [handleLineClick],
  );

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
          hunks={parsedFile.hunks}
          widgets={widgets}
          gutterType="default"
          gutterEvents={lineEvents}
          codeEvents={lineEvents}
          optimizeSelection={false}
          className="!text-sm"
        >
          {(hunks) =>
            hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)
          }
        </Diff>
      </div>
      <p className="mt-3 font-body text-xs text-text-muted">
        Click a line number to add an inline comment.
      </p>
    </div>
  );
}
