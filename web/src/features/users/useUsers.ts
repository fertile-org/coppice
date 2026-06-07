import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type { CreateUserInput } from '../../lib/schemas/agent';

export const USERS_QUERY_KEY = ['users'] as const;

export interface WorkspaceUser {
  id: string;
  email: string;
  role: string;
  createdAt: string;
}

async function fetchUsers(): Promise<WorkspaceUser[]> {
  const res = await apiFetch('/api/users');
  const data = (await res.json()) as { items: WorkspaceUser[] };
  return data.items;
}

async function createUser(body: CreateUserInput): Promise<WorkspaceUser> {
  const res = await apiFetch('/api/users', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<WorkspaceUser>;
}

export function useUsers() {
  return useQuery({
    queryKey: USERS_QUERY_KEY,
    queryFn: fetchUsers,
  });
}

export function useCreateUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createUser,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: USERS_QUERY_KEY });
    },
  });
}
