import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { ApiError } from '../../lib/api';
import {
  getLastProjectId,
  setLastProjectId,
  useCreateProject,
  useProjects,
  type Project,
} from './useProjects';

function formatCreatedAt(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

function ProjectCard({
  project,
  isRecent,
  onSelect,
}: {
  project: Project;
  isRecent: boolean;
  onSelect: (project: Project) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(project)}
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
        {project.name}
      </h2>
      <p className="mt-1 font-mono text-xs text-text-muted">{project.slug}</p>
      {project.createdAt && (
        <p className="mt-4 font-body text-xs text-text-secondary">
          Created {formatCreatedAt(project.createdAt)}
        </p>
      )}
    </button>
  );
}

function NewProjectDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const nameRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const createProject = useCreateProject();
  const navigate = useNavigate();

  useEffect(() => {
    if (open) {
      setName('');
      setError(null);
      const timer = window.setTimeout(() => nameRef.current?.focus(), 0);
      return () => window.clearTimeout(timer);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }

    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Project name is required.');
      return;
    }

    setError(null);
    try {
      const project = await createProject.mutateAsync(trimmed);
      setLastProjectId(project.id);
      onClose();
      navigate(`/projects/${project.id}/board`);
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) {
        setError('Invalid project name.');
      } else {
        setError('Unable to create project. Please try again.');
      }
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-bark-950/40 px-4"
      role="presentation"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-project-title"
        className="w-full max-w-md rounded-xl border border-border bg-paper-50 p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          id="new-project-title"
          className="font-display text-xl font-semibold text-bark-900"
        >
          New project
        </h2>
        <p className="mt-1 font-body text-sm text-text-secondary">
          Give your workspace a name to get started.
        </p>

        <form onSubmit={(e) => void handleSubmit(e)} className="mt-5 space-y-4">
          <div>
            <label
              htmlFor="project-name"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Name
            </label>
            <input
              ref={nameRef}
              id="project-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Coppice Platform"
              className="w-full rounded-md border border-border bg-surface-raised px-3 py-2 font-body text-sm text-text-primary outline-none transition-colors duration-fast focus:border-moss-500 focus:ring-2 focus:ring-moss-100"
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
              disabled={createProject.isPending}
              className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-bark-300 hover:text-text-primary disabled:opacity-60"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createProject.isPending}
              className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
            >
              {createProject.isPending ? 'Creating…' : 'Create project'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export function ProjectPickerPage() {
  const navigate = useNavigate();
  const { data: projects, isLoading, isError, refetch } = useProjects();
  const [dialogOpen, setDialogOpen] = useState(false);
  const lastProjectId = getLastProjectId();

  function handleSelectProject(project: Project) {
    setLastProjectId(project.id);
    navigate(`/projects/${project.id}/board`);
  }

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-display text-2xl font-semibold text-bark-900">
            Projects
          </h1>
          <p className="mt-2 max-w-xl font-body text-text-secondary">
            Select or create a project to open its board.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setDialogOpen(true)}
          className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 shadow-sm transition-colors duration-fast hover:bg-moss-700"
        >
          New project
        </button>
      </div>

      {isLoading && (
        <p className="mt-10 font-body text-sm text-text-muted">
          Loading projects…
        </p>
      )}

      {isError && (
        <div className="mt-10 rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
          <p className="font-body text-sm text-danger">
            Unable to load projects.
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

      {!isLoading && !isError && projects?.length === 0 && (
        <div className="mt-10 rounded-xl border border-dashed border-bark-300 bg-paper-100 px-8 py-12 text-center">
          <p className="font-display text-lg font-semibold text-bark-800">
            No projects yet
          </p>
          <p className="mt-2 font-body text-sm text-text-secondary">
            Create your first project to open a board.
          </p>
          <button
            type="button"
            onClick={() => setDialogOpen(true)}
            className="mt-6 rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700"
          >
            Create project
          </button>
        </div>
      )}

      {!isLoading && !isError && projects && projects.length > 0 && (
        <ul className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {projects.map((project) => (
            <li key={project.id}>
              <ProjectCard
                project={project}
                isRecent={project.id === lastProjectId}
                onSelect={handleSelectProject}
              />
            </li>
          ))}
        </ul>
      )}

      <NewProjectDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
      />
    </div>
  );
}
