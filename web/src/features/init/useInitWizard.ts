import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { ApiError, apiFetch } from '../../lib/api';
import { AGENTS_QUERY_KEY, CONNECTORS_QUERY_KEY } from '../agents/useAgents';
import { BOARDS_QUERY_KEY } from '../boards/useBoards';
import { REPOS_QUERY_KEY } from '../repos/useRepos';

const memberSchema = z.object({
  presetKey: z.string(),
  name: z.string(),
  role: z.string(),
});

export const initPreviewSchema = z.object({
  presetId: z.literal('starter'),
  status: z.enum(['ready', 'checking', 'none']),
  connector: z
    .object({
      id: z.string(),
      displayName: z.string(),
    })
    .nullable(),
  members: z.array(memberSchema),
});

export type InitPreview = z.infer<typeof initPreviewSchema>;

const confirmResultSchema = z.object({
  boardId: z.string().uuid(),
  repositoryId: z.string().uuid(),
  connectorId: z.string(),
  connectorDisplayName: z.string(),
  connectorTurnedOn: z.boolean(),
  agents: z.array(
    z.object({
      id: z.string().uuid(),
      name: z.string(),
      presetKey: z.string(),
    }),
  ),
});

export type InitConfirmResult = z.infer<typeof confirmResultSchema>;

export interface InitConfirmInput {
  boardName: string;
  repository: {
    name: string;
    localPath: string;
    remoteUrl: string | null;
    defaultBranch: string;
  };
}

export const INIT_PREVIEW_QUERY_KEY = ['init-wizard'] as const;

async function fetchPreview(): Promise<InitPreview> {
  const res = await apiFetch('/api/init');
  return initPreviewSchema.parse(await res.json());
}

async function confirmInit(input: InitConfirmInput): Promise<InitConfirmResult> {
  const res = await apiFetch('/api/init', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      presetId: 'starter',
      boardName: input.boardName,
      repository: input.repository,
    }),
  });
  return confirmResultSchema.parse(await res.json());
}

export function initErrorCode(err: unknown): string | null {
  if (!(err instanceof ApiError)) return null;
  try {
    const parsed = JSON.parse(err.body) as { code?: unknown };
    return typeof parsed.code === 'string' ? parsed.code : null;
  } catch {
    return null;
  }
}

export function useInitPreview(enabled: boolean) {
  return useQuery({
    queryKey: INIT_PREVIEW_QUERY_KEY,
    queryFn: fetchPreview,
    enabled,
    refetchInterval: (query) =>
      query.state.data?.status === 'checking' ? 2_000 : false,
  });
}

export function useConfirmInit() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: confirmInit,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: AGENTS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: CONNECTORS_QUERY_KEY });
    },
  });
}
