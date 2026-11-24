import { Navigate } from 'react-router-dom';
import { setLastBoardId, useBoards } from './useBoards';

/** Entry at `/`: wait for boards, then open the only board or fall through to `/boards`. */
export function BoardEntryRedirect() {
  const { data: boards, isPending, isError } = useBoards();

  if (isPending) {
    return (
      <p className="font-body text-sm text-text-muted">Loading boards…</p>
    );
  }

  if (!isError && boards?.length === 1) {
    const board = boards[0];
    setLastBoardId(board.id);
    return <Navigate to={`/boards/${board.id}`} replace />;
  }

  return <Navigate to="/boards" replace />;
}
