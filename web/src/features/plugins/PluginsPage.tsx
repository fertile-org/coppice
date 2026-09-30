import { ArrowDown, ArrowUp } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { Navigate } from 'react-router-dom';
import { Button } from '../../components/ui/button';
import { isDesktopShell, pickDirectory } from '../../lib/desktop';
import type { PluginDir } from '../../lib/schemas/plugin';
import { useSession } from '../auth/useSession';
import { PluginCard } from './PluginCard';
import { pluginErrorMessage } from './pluginError';
import {
  useAddPluginDir,
  useInstallPlugin,
  useMovePluginDir,
  usePluginDirs,
  usePluginInstall,
  usePlugins,
  useRemovePluginDir,
  useRescanPlugins,
} from './usePlugins';

const ERROR_CLASS =
  'rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger';

function SectionHeading({ children }: { children: string }) {
  return (
    <h2 className="font-display text-lg font-semibold text-bark-900">{children}</h2>
  );
}

function PluginDirsSection({ dirs }: { dirs: PluginDir[] }) {
  const [path, setPath] = useState('');
  const [desktop] = useState(isDesktopShell);
  const [error, setError] = useState<string | null>(null);
  const addDir = useAddPluginDir();
  const moveDir = useMovePluginDir();
  const removeDir = useRemovePluginDir();
  const rescan = useRescanPlugins();

  async function run(action: () => Promise<unknown>, fallback: string) {
    setError(null);
    try {
      await action();
      return true;
    } catch (err) {
      setError(pluginErrorMessage(err, fallback));
      return false;
    }
  }

  async function handleAdd(e: FormEvent) {
    e.preventDefault();
    const trimmed = path.trim();
    if (!trimmed) {
      setError('Directory path is required.');
      return;
    }
    if (await run(() => addDir.mutateAsync(trimmed), 'Unable to add directory.')) {
      setPath('');
    }
  }

  async function handleBrowse() {
    const selected = await pickDirectory();
    if (selected) setPath(selected);
  }

  function handleRemove(dir: PluginDir) {
    if (!window.confirm(`Remove plugin directory "${dir.path}"?`)) return;
    void run(() => removeDir.mutateAsync(dir.id), 'Unable to remove directory.');
  }

  const busy = moveDir.isPending || removeDir.isPending;

  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <SectionHeading>Plugin directories</SectionHeading>
          <p className="mt-1 font-body text-sm text-text-secondary">
            Scanned in order; a plugin in an earlier directory shadows one with the
            same name later.
          </p>
        </div>
        <Button
          type="button"
          variant="secondary"
          loading={rescan.isPending}
          onClick={() => void run(() => rescan.mutateAsync(), 'Rescan failed.')}
        >
          Rescan
        </Button>
      </div>

      <ol className="overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
        {dirs.map((dir, index) => (
          <li
            key={dir.id}
            data-testid={`plugin-dir-${dir.id}`}
            className="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0"
          >
            <span className="w-5 font-body text-xs text-text-muted">{index + 1}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-secondary">
              {dir.path}
            </span>
            {dir.isDefault && (
              <span className="rounded-full border border-border px-2 py-0.5 font-body text-xs text-text-muted">
                Default
              </span>
            )}
            <div className="flex items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={`Move ${dir.path} up`}
                disabled={index === 0 || busy}
                onClick={() =>
                  void run(
                    () => moveDir.mutateAsync({ id: dir.id, position: index - 1 }),
                    'Unable to reorder directories.',
                  )
                }
              >
                <ArrowUp className="h-3.5 w-3.5" aria-hidden />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={`Move ${dir.path} down`}
                disabled={index === dirs.length - 1 || busy}
                onClick={() =>
                  void run(
                    () => moveDir.mutateAsync({ id: dir.id, position: index + 1 }),
                    'Unable to reorder directories.',
                  )
                }
              >
                <ArrowDown className="h-3.5 w-3.5" aria-hidden />
              </Button>
              {!dir.isDefault && (
                <Button
                  type="button"
                  variant="destructive"
                  size="sm"
                  aria-label={`Remove ${dir.path}`}
                  disabled={busy}
                  onClick={() => handleRemove(dir)}
                >
                  Remove
                </Button>
              )}
            </div>
          </li>
        ))}
      </ol>

      <form onSubmit={(e) => void handleAdd(e)} className="flex flex-wrap items-end gap-2">
        <div className="min-w-[16rem] flex-1">
          <label
            htmlFor="plugin-dir-path"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Directory path
          </label>
          <input
            id="plugin-dir-path"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="/srv/coppice-plugins"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        {desktop && (
          <Button type="button" variant="secondary" onClick={() => void handleBrowse()}>
            Browse…
          </Button>
        )}
        <Button type="submit" loading={addDir.isPending}>
          Add directory
        </Button>
      </form>

      {error && (
        <p role="alert" className={ERROR_CLASS}>
          {error}
        </p>
      )}
    </section>
  );
}

function InstallSection({ dirs }: { dirs: PluginDir[] }) {
  const [gitUrl, setGitUrl] = useState('');
  const [ref, setRef] = useState('');
  const [pluginDirId, setPluginDirId] = useState('');
  const [installId, setInstallId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const installPlugin = useInstallPlugin();
  const { data: install } = usePluginInstall(installId);
  const targetDirId = dirs.some((dir) => dir.id === pluginDirId)
    ? pluginDirId
    : (dirs[0]?.id ?? '');
  const running = installPlugin.isPending || install?.status === 'running';

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const url = gitUrl.trim();
    if (!url) {
      setError('Git URL is required.');
      return;
    }
    if (!targetDirId) {
      setError('Add a plugin directory first.');
      return;
    }
    setError(null);
    setInstallId(null);
    try {
      const started = await installPlugin.mutateAsync({
        gitUrl: url,
        ref: ref.trim() || undefined,
        pluginDirId: targetDirId,
      });
      setInstallId(started.id);
    } catch (err) {
      setError(pluginErrorMessage(err, 'Unable to start install.'));
    }
  }

  return (
    <section className="space-y-3">
      <SectionHeading>Install from git</SectionHeading>
      <form
        onSubmit={(e) => void handleSubmit(e)}
        className="grid gap-3 rounded-xl border border-border bg-surface-raised p-4 shadow-card sm:grid-cols-[2fr_1fr_1fr_auto] sm:items-end"
      >
        <div>
          <label
            htmlFor="plugin-git-url"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Git URL
          </label>
          <input
            id="plugin-git-url"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="https://github.com/org/plugin.git"
            value={gitUrl}
            onChange={(e) => setGitUrl(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="plugin-git-ref"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Ref <span className="font-normal text-text-muted">(optional)</span>
          </label>
          <input
            id="plugin-git-ref"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="main"
            value={ref}
            onChange={(e) => setRef(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="plugin-target-dir"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Target directory
          </label>
          <select
            id="plugin-target-dir"
            value={targetDirId}
            onChange={(e) => setPluginDirId(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          >
            {dirs.map((dir) => (
              <option key={dir.id} value={dir.id}>
                {dir.path}
              </option>
            ))}
          </select>
        </div>
        <Button type="submit" loading={running}>
          Install
        </Button>
      </form>

      {install?.status === 'running' && (
        <p className="font-body text-sm text-text-muted">
          Installing {install.gitUrl}…
        </p>
      )}
      {install?.status === 'succeeded' && (
        <p className="font-body text-sm text-success">Installed {install.gitUrl}.</p>
      )}
      {install?.status === 'failed' && (
        <p role="alert" className={ERROR_CLASS}>
          Install failed: {install.error ?? 'unknown error'}
        </p>
      )}
      {error && (
        <p role="alert" className={ERROR_CLASS}>
          {error}
        </p>
      )}
    </section>
  );
}

export function PluginsPage() {
  const { user, loading } = useSession();
  const dirsQuery = usePluginDirs();
  const pluginsQuery = usePlugins();

  if (loading) {
    return <p className="font-body text-sm text-text-muted">Loading session…</p>;
  }

  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }

  const dirs = dirsQuery.data ?? [];
  const plugins = pluginsQuery.data ?? [];
  const isLoading = dirsQuery.isLoading || pluginsQuery.isLoading;
  const isError = dirsQuery.isError || pluginsQuery.isError;

  return (
    <div className="space-y-8">
      <div>
        <h1 className="font-display text-2xl font-semibold text-bark-900">Plugins</h1>
        <p className="mt-2 max-w-xl font-body text-text-secondary">
          Skill bundles scanned from plugin directories. Enable a plugin, then attach
          it to agents.
        </p>
      </div>

      {isLoading && (
        <p className="font-body text-sm text-text-muted">Loading plugins…</p>
      )}

      {isError && (
        <div className="rounded-lg border border-danger-muted bg-danger-muted/50 p-4">
          <p className="font-body text-sm text-danger">Unable to load plugins.</p>
          <button
            type="button"
            onClick={() => {
              void dirsQuery.refetch();
              void pluginsQuery.refetch();
            }}
            className="mt-2 font-body text-sm font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Try again
          </button>
        </div>
      )}

      {!isLoading && !isError && (
        <>
          <PluginDirsSection dirs={dirs} />
          <InstallSection dirs={dirs} />
          <section className="space-y-3">
            <SectionHeading>Installed plugins</SectionHeading>
            {plugins.length === 0 ? (
              <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
                No plugins found. Add a directory or install one from git.
              </p>
            ) : (
              <div className="space-y-3">
                {plugins.map((plugin) => (
                  <PluginCard key={plugin.id} plugin={plugin} />
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
