import { useEffect, useState, type FormEvent } from 'react';
import { Button } from '../../components/ui/button';
import { ApiError } from '../../lib/api';
import {
  createRepoSchema,
  updateRepoSchema,
  type Repo,
  type VerificationStatus,
} from '../../lib/schemas/repo';
import { useSession } from '../auth/useSession';
import { DefaultBranchSyncControls } from './DefaultBranchSyncControls';
import { RepoDrawer } from './RepoDrawer';
import {
  useClearForgeToken,
  useCreateRepo,
  useDeleteRepo,
  useRepos,
  useSetForgeToken,
  useUpdateRepo,
  useVerifyRepo,
} from './useRepos';

const STATUS_LABELS: Record<VerificationStatus, string> = {
  ready: 'Ready',
  path_missing: 'Path missing',
  not_git_repo: 'Not a git repo',
  error: 'Error',
};

type DrawerState =
  | { type: 'closed' }
  | { type: 'create' }
  | { type: 'edit'; repoId: string };

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

interface CreateRepoFormProps {
  onCreated: () => void;
}

function CreateRepoForm({ onCreated }: CreateRepoFormProps) {
  const [name, setName] = useState('');
  const [localPath, setLocalPath] = useState('');
  const [remoteUrl, setRemoteUrl] = useState('');
  const [defaultBranch, setDefaultBranch] = useState('main');
  const [error, setError] = useState<string | null>(null);
  const createRepo = useCreateRepo();

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
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
      onCreated();
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
    <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
      <RepoFields
        name={name}
        localPath={localPath}
        remoteUrl={remoteUrl}
        defaultBranch={defaultBranch}
        onNameChange={setName}
        onLocalPathChange={setLocalPath}
        onRemoteUrlChange={setRemoteUrl}
        onDefaultBranchChange={setDefaultBranch}
        idPrefix="create"
      />
      {error && (
        <p
          role="alert"
          className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}
      <button
        type="submit"
        disabled={createRepo.isPending}
        className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
      >
        {createRepo.isPending ? 'Creating…' : 'Add repository'}
      </button>
    </form>
  );
}

interface EditRepoFormProps {
  repo: Repo;
}

function EditRepoForm({ repo }: EditRepoFormProps) {
  const [name, setName] = useState(repo.name);
  const [localPath, setLocalPath] = useState(repo.localPath);
  const [remoteUrl, setRemoteUrl] = useState(repo.remoteUrl ?? '');
  const [defaultBranch, setDefaultBranch] = useState(repo.defaultBranch);
  const [error, setError] = useState<string | null>(null);
  const updateRepo = useUpdateRepo();

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
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
      await updateRepo.mutateAsync({ id: repo.id, ...parsed.data });
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setError('A repository with that path already exists.');
      } else if (err instanceof ApiError && err.status === 403) {
        setError('You do not have permission to update repositories.');
      } else {
        setError('Unable to update repository. Please try again.');
      }
    }
  }

  return (
    <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
      <h3 className="font-display text-sm font-semibold text-bark-800">
        Metadata
      </h3>
      <RepoFields
        name={name}
        localPath={localPath}
        remoteUrl={remoteUrl}
        defaultBranch={defaultBranch}
        onNameChange={setName}
        onLocalPathChange={setLocalPath}
        onRemoteUrlChange={setRemoteUrl}
        onDefaultBranchChange={setDefaultBranch}
        idPrefix="edit"
      />
      {error && (
        <p
          role="alert"
          className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {error}
        </p>
      )}
      <button
        type="submit"
        disabled={updateRepo.isPending}
        className="rounded-md bg-moss-600 px-4 py-2 font-body text-sm font-medium text-paper-50 transition-colors duration-fast hover:bg-moss-700 disabled:opacity-60"
      >
        {updateRepo.isPending ? 'Saving…' : 'Save changes'}
      </button>
    </form>
  );
}

interface RepoFieldsProps {
  name: string;
  localPath: string;
  remoteUrl: string;
  defaultBranch: string;
  onNameChange: (value: string) => void;
  onLocalPathChange: (value: string) => void;
  onRemoteUrlChange: (value: string) => void;
  onDefaultBranchChange: (value: string) => void;
  idPrefix: string;
}

function RepoFields({
  name,
  localPath,
  remoteUrl,
  defaultBranch,
  onNameChange,
  onLocalPathChange,
  onRemoteUrlChange,
  onDefaultBranchChange,
  idPrefix,
}: RepoFieldsProps) {
  return (
    <div className="space-y-3">
      <div>
        <label
          htmlFor={`${idPrefix}-repo-name`}
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Name
        </label>
        <input
          id={`${idPrefix}-repo-name`}
          type="text"
          required
          autoComplete="off"
          value={name}
          onChange={(e) => onNameChange(e.target.value)}
          className="field-control w-full px-3 py-2 font-body text-sm"
        />
      </div>

      <div>
        <label
          htmlFor={`${idPrefix}-repo-local-path`}
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Local path
        </label>
        <input
          id={`${idPrefix}-repo-local-path`}
          type="text"
          required
          autoComplete="off"
          placeholder="/repos/my-app"
          value={localPath}
          onChange={(e) => onLocalPathChange(e.target.value)}
          className="field-control w-full px-3 py-2 font-mono text-sm"
        />
      </div>

      <div>
        <label
          htmlFor={`${idPrefix}-repo-remote-url`}
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Remote URL{' '}
          <span className="font-normal text-text-muted">(optional)</span>
        </label>
        <input
          id={`${idPrefix}-repo-remote-url`}
          type="url"
          autoComplete="off"
          placeholder="https://github.com/org/repo.git"
          value={remoteUrl}
          onChange={(e) => onRemoteUrlChange(e.target.value)}
          className="field-control w-full px-3 py-2 font-mono text-sm"
        />
      </div>

      <div>
        <label
          htmlFor={`${idPrefix}-repo-default-branch`}
          className="mb-1 block font-body text-sm font-medium text-bark-800"
        >
          Default branch
        </label>
        <input
          id={`${idPrefix}-repo-default-branch`}
          type="text"
          required
          autoComplete="off"
          value={defaultBranch}
          onChange={(e) => onDefaultBranchChange(e.target.value)}
          className="field-control w-full px-3 py-2 font-body text-sm"
        />
      </div>
    </div>
  );
}

function ForgeTokenSection({ repo }: { repo: Repo }) {
  const setToken = useSetForgeToken();
  const clearToken = useClearForgeToken();
  const [token, setTokenValue] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleSave(e: FormEvent) {
    e.preventDefault();
    if (!token.trim()) {
      setError('Paste a forge token.');
      return;
    }
    setError(null);
    setMessage(null);
    try {
      await setToken.mutateAsync({ id: repo.id, token: token.trim() });
      setTokenValue('');
      setMessage('Forge token saved. The value is not shown again.');
    } catch {
      setError('Unable to save forge token.');
    }
  }

  async function handleClear() {
    if (!repo.forgeTokenConfigured) return;
    if (!window.confirm('Remove the forge token for this repository?')) return;
    setError(null);
    setMessage(null);
    try {
      await clearToken.mutateAsync(repo.id);
      setMessage('Forge token removed.');
    } catch {
      setError('Unable to remove forge token.');
    }
  }

  return (
    <div className="space-y-3 border-t border-border pt-5">
      <div>
        <h3 className="font-display text-sm font-semibold text-bark-800">
          Forge token
        </h3>
        <p className="mt-1 font-body text-sm text-text-muted">
          GitHub PAT (or fine-grained token) for human-triggered push and create
          PR. Value is stored encrypted and never shown again.
        </p>
      </div>
      <form onSubmit={(e) => void handleSave(e)} className="space-y-3">
        <div>
          <label
            htmlFor="forge-token"
            className="font-body text-xs font-medium text-text-muted"
          >
            Token
          </label>
          <input
            id="forge-token"
            type="password"
            autoComplete="off"
            value={token}
            onChange={(e) => setTokenValue(e.target.value)}
            placeholder="ghp_…"
            className="field-control mt-1 w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <p className="font-body text-xs text-text-muted">
          Status:{' '}
          {repo.forgeTokenConfigured ? (
            <span className="text-success">configured</span>
          ) : (
            <span className="text-warning">not configured</span>
          )}
        </p>
        {error && <p className="font-body text-xs text-danger">{error}</p>}
        {message && <p className="font-body text-xs text-success">{message}</p>}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={setToken.isPending}>
            {setToken.isPending ? 'Saving…' : 'Save token'}
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={!repo.forgeTokenConfigured || clearToken.isPending}
            onClick={() => void handleClear()}
          >
            {clearToken.isPending ? 'Removing…' : 'Clear'}
          </Button>
        </div>
      </form>
    </div>
  );
}

interface EditRepoActionsProps {
  repo: Repo;
  onRemoved: () => void;
}

function EditRepoActions({ repo, onRemoved }: EditRepoActionsProps) {
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
      onRemoved();
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setActionError('Repository is in use by a ticket.');
      } else {
        setActionError('Unable to delete repository.');
      }
    }
  }

  return (
    <div className="space-y-3 border-t border-border pt-5">
      <h3 className="font-display text-sm font-semibold text-bark-800">
        Actions
      </h3>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void handleVerify()}
          disabled={verifyRepo.isPending}
          className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-moss-500 hover:text-moss-700 disabled:opacity-50"
        >
          {verifyRepo.isPending ? 'Verifying…' : 'Verify'}
        </button>
        <button
          type="button"
          onClick={() => void handleDelete()}
          disabled={deleteRepo.isPending}
          className="rounded-md border border-danger-muted px-3 py-1.5 font-body text-sm text-danger transition-colors duration-fast hover:bg-danger-muted/40 disabled:opacity-50"
        >
          {deleteRepo.isPending ? 'Removing…' : 'Remove'}
        </button>
      </div>
      {actionError && (
        <p className="font-body text-sm text-danger">{actionError}</p>
      )}
      <DefaultBranchSyncControls repo={repo} />
    </div>
  );
}

export function RepositoriesPage() {
  const { user, loading } = useSession();
  const { data: repos, isLoading, isError, refetch } = useRepos();
  const [drawer, setDrawer] = useState<DrawerState>({ type: 'closed' });
  const isAdmin = user?.role === 'admin';

  const editingRepo =
    drawer.type === 'edit'
      ? (repos?.find((repo) => repo.id === drawer.repoId) ?? null)
      : null;

  useEffect(() => {
    if (drawer.type === 'edit' && repos && !editingRepo) {
      setDrawer({ type: 'closed' });
    }
  }, [drawer, repos, editingRepo]);

  function closeDrawer() {
    setDrawer({ type: 'closed' });
  }

  if (loading) {
    return (
      <p className="font-body text-sm text-text-muted">Loading session…</p>
    );
  }

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-4">
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
        {isAdmin && (
          <Button
            type="button"
            onClick={() => setDrawer({ type: 'create' })}
          >
            Add repository
          </Button>
        )}
      </div>

      <div className="mt-8">
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
                    <th className="px-4 py-3 text-right font-body text-xs font-medium uppercase tracking-wide text-text-muted">
                      Actions
                    </th>
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
                        <div className="mt-1">
                          <span
                            className={[
                              'inline-flex items-center rounded-full border px-2 py-0.5 font-body text-xs',
                              repo.forgeTokenConfigured
                                ? 'border-success-muted bg-success-muted text-success'
                                : 'border-border bg-paper-100 text-text-muted',
                            ].join(' ')}
                          >
                            {repo.forgeTokenConfigured
                              ? 'token configured'
                              : 'token not configured'}
                          </span>
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
                      <td className="px-4 py-3">
                        <div className="flex flex-wrap items-center justify-end gap-2">
                          <Button
                            type="button"
                            variant="secondary"
                            disabled={repo.verificationStatus !== 'ready'}
                            onClick={() => {
                              window.open(
                                `/code?repoId=${repo.id}`,
                                '_blank',
                                'noopener,noreferrer',
                              );
                            }}
                          >
                            View code
                          </Button>
                          {isAdmin && (
                            <button
                              type="button"
                              onClick={() =>
                                setDrawer({ type: 'edit', repoId: repo.id })
                              }
                              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
                            >
                              Edit
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>

      {isAdmin && drawer.type === 'create' && (
        <RepoDrawer
          ariaLabel="Add repository"
          title="Add repository"
          description="Register a local git checkout on the server."
          onClose={closeDrawer}
        >
          <CreateRepoForm onCreated={closeDrawer} />
        </RepoDrawer>
      )}

      {isAdmin && drawer.type === 'edit' && editingRepo && (
        <RepoDrawer
          ariaLabel="Edit repository"
          title="Edit repository"
          description="Update metadata, forge token, and admin actions for this checkout."
          onClose={closeDrawer}
        >
          <div className="space-y-6">
            <EditRepoForm key={editingRepo.id} repo={editingRepo} />
            <ForgeTokenSection key={`token-${editingRepo.id}`} repo={editingRepo} />
            <EditRepoActions repo={editingRepo} onRemoved={closeDrawer} />
          </div>
        </RepoDrawer>
      )}
    </div>
  );
}
