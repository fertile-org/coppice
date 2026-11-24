import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { InitWizardOffer } from '../init/InitWizardOffer';
import { NewBoardDialog } from './NewBoardDialog';
import {
  getLastBoardId,
  setLastBoardId,
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
      <InitWizardOffer />
    </div>
  );
}
