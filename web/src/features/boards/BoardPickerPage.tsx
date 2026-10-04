import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { ApiError } from '../../lib/api';
import {
  getLastBoardId,
  setLastBoardId,
  useCreateBoard,
  useBoards,
  type Board,
} from './useBoards';

function formatCreatedAt(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

function BoardCard({
  board,
  isRecent,
  onSelect,
}: {
  board: Board;
  isRecent: boolean;
  onSelect: (board: Board) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(board)}
      className={[
        'group flex w-full flex-col rounded-lg border bg-surface-raised p-5 text-left shadow-card transition-all duration-fast',
        'hover:border-moss-400 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted',
        isRecent
          ? 'border-moss-400 ring-1 ring-moss-200'
          : 'border-border',
      ].join(' ')}
    >
      <div className="flex items-start justify-between gap-3">
        <span
          className="mt-0.5 inline-block h-2.5 w-2.5 shrink-0 rounded-full bg-moss-500 transition-colors duration-fast group-hover:bg-moss-600"
          aria-hidden="true"
        />
        {isRecent && (
          <span className="rounded-full bg-moss-100 px-2 py-0.5 font-body text-xs font-medium text-moss-800">
            Recent
          </span>
        )}
      </div>
      <h2 className="mt-3 font-display text-lg font-semibold text-bark-900 group-hover:text-moss-800">
        {board.name}
      </h2>
      <p className="mt-1 font-mono text-xs text-text-muted">{board.slug}</p>
      {board.createdAt && (
        <p className="mt-4 font-body text-xs text-text-secondary">
          Created {formatCreatedAt(board.createdAt)}
        </p>
      )}
    </button>
  );
}

function NewBoardDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const nameRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const createBoard = useCreateBoard();
  const navigate = useNavigate();

  useEffect(() => {
    if (open) {
      setName('');
      setError(null);
      const timer = window.setTimeout(() => nameRef.current?.focus(), 0);
      return () => window.clearTimeout(timer);
    }
  }, [open]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Board name is required.');
      return;
    }

    setError(null);
    try {
      const board = await createBoard.mutateAsync(trimmed);
      setLastBoardId(board.id);
      onClose();
      navigate(`/boards/${board.id}`);
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) {
        setError('Invalid board name.');
      } else {
        setError('Unable to create board. Please try again.');
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="max-w-md">
        <DialogTitle>New board</DialogTitle>
        <DialogDescription className="mt-1">
          Give your workspace a name to get started.
        </DialogDescription>

        <form onSubmit={(e) => void handleSubmit(e)} className="mt-5 space-y-4">
          <div>
            <label
              htmlFor="board-name"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Name
            </label>
            <input
              ref={nameRef}
              id="board-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Coppice Platform"
              className="field-control w-full px-3 py-2 font-body text-sm"
            />
          </div>

          {error && (
            <p
              role="alert"
              className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
            >
              {error}
            </p>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              disabled={createBoard.isPending}
              className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-bark-300 hover:text-text-primary disabled:opacity-60"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createBoard.isPending}
              className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
            >
              {createBoard.isPending ? 'Creating…' : 'Create board'}
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function BoardPickerPage() {
  const navigate = useNavigate();
  const { data: boards, isLoading, isError, refetch } = useBoards();
  const [dialogOpen, setDialogOpen] = useState(false);
  const lastBoardId = getLastBoardId();

  function handleSelectBoard(board: Board) {
    setLastBoardId(board.id);
    navigate(`/boards/${board.id}`);
  }

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-display text-2xl font-semibold text-bark-900">
            Boards
          </h1>
          <p className="mt-2 max-w-xl font-body text-text-secondary">
            Select or create a board to get started.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setDialogOpen(true)}
          className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm transition-colors duration-fast hover:bg-moss-700"
        >
          New board
        </button>
      </div>

      {isLoading && (
        <p className="mt-10 font-body text-sm text-text-muted">
          Loading boards…
        </p>
      )}

      {isError && (
        <div className="mt-10 rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
          <p className="font-body text-sm text-danger">
            Unable to load boards.
          </p>
          <button
            type="button"
            onClick={() => void refetch()}
            className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Try again
          </button>
        </div>
      )}

      {!isLoading && !isError && boards?.length === 0 && (
        <div className="mt-10 rounded-xl border border-dashed border-bark-300 bg-paper-100 px-8 py-12 text-center">
          <p className="font-display text-lg font-semibold text-bark-800">
            No boards yet
          </p>
          <p className="mt-2 font-body text-sm text-text-secondary">
            Create your first board to start tracking tickets.
          </p>
          <button
            type="button"
            onClick={() => setDialogOpen(true)}
            className="mt-6 rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700"
          >
            Create board
          </button>
        </div>
      )}

      {!isLoading && !isError && boards && boards.length > 0 && (
        <ul className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {boards.map((board) => (
            <li key={board.id}>
              <BoardCard
                board={board}
                isRecent={board.id === lastBoardId}
                onSelect={handleSelectBoard}
              />
            </li>
          ))}
        </ul>
      )}

      <NewBoardDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
      />
    </div>
  );
}
