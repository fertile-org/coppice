import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';

export const BOARDS_QUERY_KEY = ['boards'] as const;
export const LAST_BOARD_ID_KEY = 'coppice:lastBoardId';

export interface Board {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
}

export function getLastBoardId(): string | null {
  try {
    return localStorage.getItem(LAST_BOARD_ID_KEY);
  } catch {
    return null;
  }
}

export function setLastBoardId(id: string): void {
  try {
    localStorage.setItem(LAST_BOARD_ID_KEY, id);
  } catch {
    // ignore storage failures (private mode, quota, etc.)
  }
}

async function fetchBoards(): Promise<Board[]> {
  const res = await apiFetch('/api/boards');
  return res.json() as Promise<Board[]>;
}

async function createBoard(name: string): Promise<Board> {
  const res = await apiFetch('/api/boards', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  return res.json() as Promise<Board>;
}

export function useBoards() {
  return useQuery({
    queryKey: BOARDS_QUERY_KEY,
    queryFn: fetchBoards,
  });
}

export function useCreateBoard() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createBoard,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}
