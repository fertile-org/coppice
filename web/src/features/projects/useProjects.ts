import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';

export const PROJECTS_QUERY_KEY = ['projects'] as const;
export const LAST_PROJECT_ID_KEY = 'coppice:lastProjectId';

export interface Project {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
}

export function getLastProjectId(): string | null {
  try {
    return localStorage.getItem(LAST_PROJECT_ID_KEY);
  } catch {
    return null;
  }
}

export function setLastProjectId(id: string): void {
  try {
    localStorage.setItem(LAST_PROJECT_ID_KEY, id);
  } catch {
    // ignore storage failures (private mode, quota, etc.)
  }
}

async function fetchProjects(): Promise<Project[]> {
  const res = await apiFetch('/api/projects');
  return res.json() as Promise<Project[]>;
}

async function createProject(name: string): Promise<Project> {
  const res = await apiFetch('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  return res.json() as Promise<Project>;
}

export function useProjects() {
  return useQuery({
    queryKey: PROJECTS_QUERY_KEY,
    queryFn: fetchProjects,
  });
}

export function useCreateProject() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createProject,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PROJECTS_QUERY_KEY });
    },
  });
}
