import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type {
  CreateRepoInput,
  Repo,
  UpdateRepoInput,
} from '../../lib/schemas/repo';

export const REPOS_QUERY_KEY = ['repos'] as const;

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

export function useRepos() {
  return useQuery({
    queryKey: REPOS_QUERY_KEY,
    queryFn: fetchRepos,
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
