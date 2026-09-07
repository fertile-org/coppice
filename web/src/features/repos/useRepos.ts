import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type {
  CreateRepoInput,
  Repo,
  UpdateRepoInput,
} from '../../lib/schemas/repo';

export const REPOS_QUERY_KEY = ['repos'] as const;

export type DefaultBranchSyncStatus = {
  defaultBranch: string;
  localSha: string | null;
  remoteSha: string | null;
  aheadCount: number | null;
  behindCount: number | null;
  workingTreeClean: boolean;
  pushEnabled: boolean;
  forgeTokenConfigured: boolean;
  canFetch: boolean;
  canPush: boolean;
  fetchDisabledReason: string | null;
  pushDisabledReason: string | null;
};

export type PushDefaultBranchResult = {
  defaultBranch: string;
  remote: string;
  message: string;
  status: DefaultBranchSyncStatus;
};

export function defaultBranchSyncQueryKey(repoId: string) {
  return ['repos', repoId, 'default-branch-sync'] as const;
}

async function fetchRepos(): Promise<Repo[]> {
  const res = await apiFetch('/api/repos');
  return res.json() as Promise<Repo[]>;
}

async function createRepo(body: CreateRepoInput): Promise<Repo> {
  const res = await apiFetch('/api/repos', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Repo>;
}

async function updateRepo({
  id,
  ...body
}: UpdateRepoInput & { id: string }): Promise<Repo> {
  const res = await apiFetch(`/api/repos/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Repo>;
}

async function deleteRepo(id: string): Promise<void> {
  await apiFetch(`/api/repos/${id}`, { method: 'DELETE' });
}

async function verifyRepo(id: string): Promise<Repo> {
  const res = await apiFetch(`/api/repos/${id}/verify`, { method: 'POST' });
  return res.json() as Promise<Repo>;
}

async function setForgeToken({
  id,
  token,
}: {
  id: string;
  token: string;
}): Promise<Repo> {
  const res = await apiFetch(`/api/repos/${id}/forge-token`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  });
  return res.json() as Promise<Repo>;
}

async function clearForgeToken(id: string): Promise<Repo> {
  const res = await apiFetch(`/api/repos/${id}/forge-token`, {
    method: 'DELETE',
  });
  return res.json() as Promise<Repo>;
}

async function fetchDefaultBranchSync(
  repoId: string,
): Promise<DefaultBranchSyncStatus> {
  const res = await apiFetch(`/api/repos/${repoId}/default-branch-sync`);
  return res.json() as Promise<DefaultBranchSyncStatus>;
}

async function fetchRemote(repoId: string): Promise<DefaultBranchSyncStatus> {
  const res = await apiFetch(`/api/repos/${repoId}/fetch`, { method: 'POST' });
  return res.json() as Promise<DefaultBranchSyncStatus>;
}

async function pushDefaultBranch(
  repoId: string,
): Promise<PushDefaultBranchResult> {
  const res = await apiFetch(`/api/repos/${repoId}/push-default-branch`, {
    method: 'POST',
  });
  return res.json() as Promise<PushDefaultBranchResult>;
}

export function useRepos() {
  return useQuery({
    queryKey: REPOS_QUERY_KEY,
    queryFn: fetchRepos,
  });
}

export function useDefaultBranchSync(repoId: string, enabled: boolean) {
  return useQuery({
    queryKey: defaultBranchSyncQueryKey(repoId),
    queryFn: () => fetchDefaultBranchSync(repoId),
    enabled,
  });
}

export function useFetchDefaultBranch(repoId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => fetchRemote(repoId),
    onSuccess: (status) => {
      queryClient.setQueryData(defaultBranchSyncQueryKey(repoId), status);
    },
  });
}

export function usePushDefaultBranch(repoId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => pushDefaultBranch(repoId),
    onSuccess: (result) => {
      queryClient.setQueryData(
        defaultBranchSyncQueryKey(repoId),
        result.status,
      );
    },
  });
}

export function useCreateRepo() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createRepo,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}

export function useUpdateRepo() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: updateRepo,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}

export function useDeleteRepo() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: deleteRepo,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}

export function useVerifyRepo() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: verifyRepo,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}

export function useSetForgeToken() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: setForgeToken,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}

export function useClearForgeToken() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: clearForgeToken,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: REPOS_QUERY_KEY });
    },
  });
}
