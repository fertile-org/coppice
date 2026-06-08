import { useState, type FormEvent } from 'react';
import { ApiError } from '../../lib/api';
import {
  createRepoSchema,
  updateRepoSchema,
  type Repo,
  type VerificationStatus,
} from '../../lib/schemas/repo';
import { useSession } from '../auth/useSession';
import {
  useCreateRepo,
  useDeleteRepo,
  useRepos,
  useUpdateRepo,
  useVerifyRepo,
} from './useRepos';

const STATUS_LABELS: Record<VerificationStatus, string> = {
  ready: 'Ready',
  path_missing: 'Path missing',
  not_git_repo: 'Not a git repo',
  error: 'Error',
};

function statusPillClass(status: VerificationStatus): string {
  const base =
    'inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 font-body text-xs';
  switch (status) {
    case 'ready':
      return `${base} border-success-muted bg-success-muted text-success`;
    case 'path_missing':
      return `${base} border-warning-muted bg-warning-muted text-warning`;
    case 'not_git_repo':
      return `${base} border-info-muted bg-info-muted text-info`;
    case 'error':
      return `${base} border-danger-muted bg-danger-muted/40 text-danger`;
  }
}

function formatDate(iso: string | null): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

interface RepoFormProps {
  editing: Repo | null;
  onCancelEdit: () => void;
}

function RepoForm({ editing, onCancelEdit }: RepoFormProps) {
  const [name, setName] = useState(editing?.name ?? '');
  const [localPath, setLocalPath] = useState(editing?.localPath ?? '');
  const [remoteUrl, setRemoteUrl] = useState(editing?.remoteUrl ?? '');
  const [defaultBranch, setDefaultBranch] = useState(
    editing?.defaultBranch ?? 'main',
  );
  const [error, setError] = useState<string | null>(null);

  const createRepo = useCreateRepo();
  const updateRepo = useUpdateRepo();

  const isPending = createRepo.isPending || updateRepo.isPending;
  const isEditing = editing !== null;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();

    if (isEditing) {
      const parsed = updateRepoSchema.safeParse({
        name,
        localPath,
        remoteUrl: remoteUrl.trim() === '' ? null : remoteUrl.trim(),
        defaultBranch,
      });
      if (!parsed.success) {
        setError(parsed.error.issues[0]?.message ?? 'Invalid input.');
        return;
      }

      setError(null);
      try {
        await updateRepo.mutateAsync({ id: editing.id, ...parsed.data });
        onCancelEdit();
      } catch (err) {
        if (err instanceof ApiError && err.status === 409) {
          setError('A repository with that path already exists.');
        } else if (err instanceof ApiError && err.status === 403) {
          setError('You do not have permission to update repositories.');
        } else {
          setError('Unable to update repository. Please try again.');
        }
      }
      return;
    }

    const parsed = createRepoSchema.safeParse({
      name,
      localPath,
      remoteUrl: remoteUrl.trim() === '' ? undefined : remoteUrl.trim(),
      defaultBranch,
    });
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? 'Invalid input.');
      return;
    }

    setError(null);
    try {
      await createRepo.mutateAsync(parsed.data);
      setName('');
      setLocalPath('');
      setRemoteUrl('');
      setDefaultBranch('main');
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setError('A repository with that path already exists.');
      } else if (err instanceof ApiError && err.status === 403) {
        setError('You do not have permission to create repositories.');
      } else {
        setError('Unable to create repository. Please try again.');
      }
    }
  }

  return (
    <form
      onSubmit={(e) => void handleSubmit(e)}
      className="rounded-xl border border-border bg-surface-raised p-5 shadow-card"
    >
      <h2 className="font-display text-lg font-semibold text-bark-900">
        {isEditing ? 'Edit repository' : 'Add repository'}
      </h2>
      <p className="mt-1 font-body text-sm text-text-secondary">
        {isEditing
          ? 'Update the registered checkout path and metadata.'
          : 'Register a local git checkout on the server.'}
      </p>

      <div className="mt-4 space-y-3">
        <div>
          <label
            htmlFor="repo-name"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Name
          </label>
          <input
            id="repo-name"
            type="text"
            required
            autoComplete="off"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="field-control w-full px-3 py-2 font-body text-sm"
          />
        </div>

        <div>
          <label
            htmlFor="repo-local-path"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Local path
          </label>
          <input
            id="repo-local-path"
            type="text"
            required
            autoComplete="off"
            placeholder="/data/my-repo"
            value={localPath}
            onChange={(e) => setLocalPath(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>

        <div>
          <label
            htmlFor="repo-remote-url"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Remote URL{' '}
            <span className="font-normal text-text-muted">(optional)</span>
          </label>
          <input
            id="repo-remote-url"
            type="url"
            autoComplete="off"
            placeholder="https://github.com/org/repo.git"
            value={remoteUrl}
            onChange={(e) => setRemoteUrl(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>

        <div>
          <label
            htmlFor="repo-default-branch"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Default branch
          </label>
          <input
            id="repo-default-branch"
            type="text"
            required
            autoComplete="off"
            value={defaultBranch}
            onChange={(e) => setDefaultBranch(e.target.value)}
            className="field-control w-full px-3 py-2 font-body text-sm"
          />
        </div>
      </div>

      {error && (
        <p
          role="alert"
          className="mt-3 rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}

      <div className="mt-4 flex flex-wrap gap-2">
        <button
          type="submit"
          disabled={isPending}
          className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
        >
          {isPending
            ? isEditing
              ? 'Saving…'
              : 'Creating…'
            : isEditing
              ? 'Save changes'
              : 'Add repository'}
        </button>
        {isEditing && (
          <button
            type="button"
            onClick={onCancelEdit}
            className="rounded-md border border-border px-4 py-2 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
          >
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}

function M07PlaceholderCard() {
  return (
    <div className="rounded-xl border border-dashed border-border bg-paper-50 p-5">
      <h2 className="font-display text-sm font-semibold text-bark-800">
        Pull request secrets
      </h2>
      <p className="mt-1 font-body text-sm text-text-muted">
        Secrets for pull requests — coming in M07
      </p>
    </div>
  );
}

interface RepoRowActionsProps {
  repo: Repo;
  onEdit: (repo: Repo) => void;
}

function RepoRowActions({ repo, onEdit }: RepoRowActionsProps) {
  const verifyRepo = useVerifyRepo();
  const deleteRepo = useDeleteRepo();
  const [actionError, setActionError] = useState<string | null>(null);

  async function handleVerify() {
    setActionError(null);
    try {
      await verifyRepo.mutateAsync(repo.id);
    } catch {
      setActionError('Verification failed.');
    }
  }

  async function handleDelete() {
    if (!window.confirm(`Remove repository "${repo.name}"?`)) return;
    setActionError(null);
    try {
      await deleteRepo.mutateAsync(repo.id);
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setActionError('Repository is in use by a ticket.');
      } else {
        setActionError('Unable to delete repository.');
      }
    }
  }

  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex flex-wrap justify-end gap-1">
        <button
          type="button"
          onClick={() => void handleVerify()}
          disabled={verifyRepo.isPending}
          className="rounded-md border border-border px-2 py-1 font-body text-xs text-text-secondary transition-colors duration-fast hover:border-moss-500 hover:text-moss-700 disabled:opacity-50"
        >
          {verifyRepo.isPending ? 'Verifying…' : 'Verify'}
        </button>
        <button
          type="button"
          onClick={() => onEdit(repo)}
          className="rounded-md border border-border px-2 py-1 font-body text-xs text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
        >
          Edit
        </button>
        <button
          type="button"
          onClick={() => void handleDelete()}
          disabled={deleteRepo.isPending}
          className="rounded-md border border-danger-muted px-2 py-1 font-body text-xs text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:opacity-50"
        >
          {deleteRepo.isPending ? 'Removing…' : 'Remove'}
        </button>
      </div>
      {actionError && (
        <p className="font-body text-xs text-danger">{actionError}</p>
      )}
    </div>
  );
}

export function RepositoriesPage() {
  const { user, loading } = useSession();
  const { data: repos, isLoading, isError, refetch } = useRepos();
  const [editing, setEditing] = useState<Repo | null>(null);
  const isAdmin = user?.role === 'admin';

  if (loading) {
    return (
      <p className="font-body text-sm text-text-muted">Loading session…</p>
    );
  }

  return (
    <div>
      <div>
        <h1 className="font-display text-2xl font-semibold text-bark-900">
          Repositories
        </h1>
        <p className="mt-2 max-w-xl font-body text-text-secondary">
          {isAdmin
            ? 'Register local git checkouts for agent worktrees.'
            : 'Registered git checkouts available for tickets and agent runs.'}
        </p>
      </div>

      <div
        className={[
          'mt-8 grid gap-8',
          isAdmin ? 'lg:grid-cols-[minmax(0,1fr)_320px]' : '',
        ].join(' ')}
      >
        <div>
          {isLoading && (
            <p className="font-body text-sm text-text-muted">
              Loading repositories…
            </p>
          )}

          {isError && (
            <div className="rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
              <p className="font-body text-sm text-danger">
                Unable to load repositories.
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

          {!isLoading && !isError && repos && (
            <div className="overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
              {repos.length === 0 ? (
                <p className="px-4 py-8 text-center font-body text-sm text-text-muted">
                  No repositories registered yet.
                </p>
              ) : (
                <table className="w-full text-left">
                  <thead>
                    <tr className="border-b border-border bg-paper-100">
                      <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                        Name
                      </th>
                      <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                        Local path
                      </th>
                      <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                        Status
                      </th>
                      <th className="px-4 py-3 font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                        Last verified
                      </th>
                      {isAdmin && (
                        <th className="px-4 py-3 text-right font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                          Actions
                        </th>
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {repos.map((repo) => (
                      <tr
                        key={repo.id}
                        className="border-b border-border last:border-b-0"
                      >
                        <td className="px-4 py-3">
                          <div className="font-body text-sm font-medium text-text-primary">
                            {repo.name}
                          </div>
                          {repo.remoteUrl && (
                            <div className="mt-0.5 truncate font-mono text-xs text-text-muted">
                              {repo.remoteUrl}
                            </div>
                          )}
                          <div className="mt-0.5 font-body text-xs text-text-muted">
                            Branch: {repo.defaultBranch}
                          </div>
                        </td>
                        <td className="max-w-[200px] truncate px-4 py-3 font-mono text-xs text-text-secondary">
                          {repo.localPath}
                        </td>
                        <td className="px-4 py-3">
                          <span
                            className={statusPillClass(repo.verificationStatus)}
                          >
                            {STATUS_LABELS[repo.verificationStatus]}
                          </span>
                          {repo.verificationError && (
                            <p
                              className="mt-1 max-w-xs font-body text-xs text-danger"
                              title={repo.verificationError}
                            >
                              {repo.verificationError}
                            </p>
                          )}
                        </td>
                        <td className="px-4 py-3 font-body text-xs text-text-muted">
                          {formatDate(repo.lastVerifiedAt)}
                        </td>
                        {isAdmin && (
                          <td className="px-4 py-3">
                            <RepoRowActions
                              repo={repo}
                              onEdit={(r) => setEditing(r)}
                            />
                          </td>
                        )}
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </div>

        {isAdmin && (
          <div className="space-y-6">
            <RepoForm
              key={editing?.id ?? 'create'}
              editing={editing}
              onCancelEdit={() => setEditing(null)}
            />
            <M07PlaceholderCard />
          </div>
        )}
      </div>
    </div>
  );
}
