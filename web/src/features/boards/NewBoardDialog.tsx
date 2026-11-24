import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { ApiError } from '../../lib/api';
import { setLastBoardId, useCreateBoard } from './useBoards';

export function NewBoardDialog({
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
