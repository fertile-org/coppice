import { Check, ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '../../components/ui/popover';
import { cn } from '../../lib/utils';
import { NewBoardDialog } from './NewBoardDialog';
import { setLastBoardId, useBoards } from './useBoards';

export function BoardSwitcher({
  boardId,
  boardName,
  boardsPending,
}: {
  boardId: string;
  boardName: string | undefined;
  boardsPending: boolean;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const { data: boards } = useBoards();
  const navigate = useNavigate();

  useEffect(() => {
    setMenuOpen(false);
  }, [boardId]);

  function selectBoard(id: string) {
    setLastBoardId(id);
    setMenuOpen(false);
    if (id !== boardId) {
      navigate(`/boards/${id}`);
    }
  }

  const label = boardName ?? (boardsPending ? '\u00a0' : 'Board');

  return (
    <>
      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <h1 className="m-0">
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={`Switch board, current: ${boardName ?? 'Board'}`}
              className={cn(
                '-ml-1.5 inline-flex max-w-full items-center gap-2 rounded-md px-1.5 py-0.5 text-left',
                'break-words font-display text-2xl font-semibold text-text-primary',
                'transition-colors duration-fast hover:bg-paper-200',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted',
              )}
            >
              <span className="min-w-0 break-words">{label}</span>
              <ChevronDown
                className="h-4 w-4 shrink-0 text-text-muted"
                aria-hidden="true"
              />
            </button>
          </PopoverTrigger>
        </h1>
        {/* Default role="dialog" so board shortcuts stay off while the menu has focus. */}
        <PopoverContent align="start" className="w-60 p-1.5" aria-label="Boards">
          <ul className="max-h-64 overflow-y-auto py-0.5">
            {(boards ?? []).map((board) => {
              const current = board.id === boardId;
              return (
                <li key={board.id}>
                  <button
                    type="button"
                    onClick={() => selectBoard(board.id)}
                    className={cn(
                      'flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left font-body text-sm',
                      'transition-colors duration-fast hover:bg-surface',
                      current
                        ? 'font-semibold text-moss-800'
                        : 'text-text-primary',
                    )}
                  >
                    <Check
                      className={cn(
                        'h-3.5 w-3.5 shrink-0',
                        current ? 'opacity-100' : 'opacity-0',
                      )}
                      aria-hidden="true"
                    />
                    <span className="min-w-0 truncate">{board.name}</span>
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="my-1 border-t border-border" />
          <Link
            to="/boards"
            onClick={() => setMenuOpen(false)}
            className="flex w-full rounded-md px-2.5 py-2 font-body text-sm text-text-primary transition-colors duration-fast hover:bg-surface"
          >
            All boards
          </Link>
          <button
            type="button"
            onClick={() => {
              setMenuOpen(false);
              setDialogOpen(true);
            }}
            className="flex w-full rounded-md px-2.5 py-2 font-body text-sm text-text-primary transition-colors duration-fast hover:bg-surface"
          >
            + New board
          </button>
        </PopoverContent>
      </Popover>
      <NewBoardDialog open={dialogOpen} onClose={() => setDialogOpen(false)} />
    </>
  );
}
