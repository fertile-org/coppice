import { useCallback, useEffect, useRef, useState } from 'react';
import { Navigate } from 'react-router-dom';
import { cn } from '../../lib/utils';
import { useSession } from '../auth/useSession';
import { missingSettingsNotice, PLUGIN_COPY, PLUGIN_DOCS_URL } from './copy';
import { CustomPluginsSection } from './CustomPluginsSection';
import { MarketplaceSection } from './MarketplaceSection';
import { PluginCard } from './PluginCard';
import {
  useMarketplaceEntries,
  useMarketplaceSources,
} from './useMarketplace';
import { usePluginDirs, usePlugins } from './usePlugins';

type PluginsTab = 'installed' | 'marketplace' | 'custom';

const TABS: { id: PluginsTab; label: string }[] = [
  { id: 'installed', label: 'Installed' },
  { id: 'marketplace', label: 'Marketplace' },
  { id: 'custom', label: 'Custom' },
];

function SectionHeading({ children }: { children: string }) {
  return (
    <h2 className="font-display text-lg font-semibold text-bark-900">{children}</h2>
  );
}

export function PluginsPage() {
  const { user, loading } = useSession();
  const dirsQuery = usePluginDirs();
  const pluginsQuery = usePlugins();
  const sourcesQuery = useMarketplaceSources();
  const entriesQuery = useMarketplaceEntries();
  const [gitUrl, setGitUrl] = useState('');
  const [tab, setTab] = useState<PluginsTab>('installed');
  const [notice, setNotice] = useState<string | null>(null);
  const [highlightIds, setHighlightIds] = useState<string[]>([]);
  const installRef = useRef<HTMLElement>(null);
  const focusGitForm = useRef(false);

  useEffect(() => {
    if (tab !== 'custom' || !focusGitForm.current || !installRef.current) return;
    focusGitForm.current = false;
    installRef.current.scrollIntoView?.({ behavior: 'smooth', block: 'start' });
    installRef.current.querySelector<HTMLInputElement>('#plugin-git-url')?.focus();
  }, [tab]);

  function handleInstallFromGit(url: string) {
    setGitUrl(url);
    focusGitForm.current = true;
    setTab('custom');
  }

  /** A marketplace install lands on the new plugin's card so it can be enabled. */
  const handleMarketplaceDone = useCallback((message: string, pluginIds: string[]) => {
    setNotice(message);
    setHighlightIds(pluginIds);
    setTab('installed');
  }, []);

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
  const missingKeys = plugins
    .filter((plugin) => highlightIds.includes(plugin.id))
    .flatMap((plugin) =>
      plugin.settings
        .filter((setting) => setting.source === 'missing')
        .map((setting) => setting.key),
    );
  const noticeText =
    notice && missingKeys.length > 0
      ? `${notice} ${missingSettingsNotice(missingKeys)}`
      : notice;
  const clonePluginCounts = new Map<string, number>();
  for (const plugin of plugins) {
    if (plugin.gitRoot) {
      clonePluginCounts.set(plugin.gitRoot, (clonePluginCounts.get(plugin.gitRoot) ?? 0) + 1);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="font-display text-2xl font-semibold text-bark-900">Plugins</h1>
        <p className="mt-2 max-w-xl font-body text-text-secondary">
          {PLUGIN_COPY.pageIntro}{' '}
          <a
            href={PLUGIN_DOCS_URL}
            target="_blank"
            rel="noreferrer"
            className="font-medium text-moss-700 underline-offset-2 hover:underline"
          >
            Plugin guide <span aria-hidden="true">↗</span>
          </a>
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
          <div role="tablist" aria-label="Plugins" className="flex gap-1 border-b border-border">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                id={`plugins-tab-${item.id}`}
                aria-selected={tab === item.id}
                aria-controls={`plugins-panel-${item.id}`}
                onClick={() => {
                  setTab(item.id);
                  setNotice(null);
                  setHighlightIds([]);
                }}
                className={cn(
                  '-mb-px border-b-2 px-3 py-2 font-body text-sm font-medium transition-colors',
                  tab === item.id
                    ? 'border-moss-700 text-moss-700'
                    : 'border-transparent text-text-muted hover:text-text-primary',
                )}
              >
                {item.label}
              </button>
            ))}
          </div>

          {tab === 'installed' && (
            <div
              role="tabpanel"
              id="plugins-panel-installed"
              aria-labelledby="plugins-tab-installed"
              className="space-y-8"
            >
              {noticeText && (
                <p role="status" className="font-body text-sm text-success">
                  {noticeText}
                </p>
              )}
              <section className="space-y-3">
                <SectionHeading>Installed plugins</SectionHeading>
                {plugins.length === 0 ? (
                  <p className="rounded-xl border border-border bg-surface-raised px-4 py-8 text-center font-body text-sm text-text-muted shadow-card">
                    No plugins yet. Find one in Marketplace or add your own in Custom. To try
                    one out, add the{' '}
                    <a
                      href="https://github.com/fertile-org/coppice/tree/main/examples/plugins"
                      target="_blank"
                      rel="noreferrer"
                      className="font-medium text-moss-700 underline-offset-2 hover:underline"
                    >
                      <code className="font-mono">examples/plugins</code>
                    </a>{' '}
                    folder from GitHub.
                  </p>
                ) : (
                  <div className="space-y-3">
                    {plugins.map((plugin) => (
                      <PluginCard
                        key={plugin.id}
                        plugin={plugin}
                        onInstallFromGit={handleInstallFromGit}
                        siblingCount={
                          plugin.gitRoot ? clonePluginCounts.get(plugin.gitRoot) : undefined
                        }
                        highlighted={highlightIds.includes(plugin.id)}
                      />
                    ))}
                  </div>
                )}
              </section>
            </div>
          )}

          {tab === 'marketplace' && (
            <div
              role="tabpanel"
              id="plugins-panel-marketplace"
              aria-labelledby="plugins-tab-marketplace"
              className="space-y-4"
            >
              {sourcesQuery.isError || entriesQuery.isError ? (
                <p className="font-body text-sm text-danger">Unable to load the marketplace.</p>
              ) : sourcesQuery.isLoading || entriesQuery.isLoading ? (
                <p className="font-body text-sm text-text-muted">Loading marketplace…</p>
              ) : (
                <MarketplaceSection
                  sources={sourcesQuery.data ?? []}
                  entries={entriesQuery.data ?? []}
                  onDone={handleMarketplaceDone}
                />
              )}
            </div>
          )}

          {tab === 'custom' && (
            <div
              role="tabpanel"
              id="plugins-panel-custom"
              aria-labelledby="plugins-tab-custom"
            >
              <CustomPluginsSection
                dirs={dirs}
                plugins={plugins}
                gitUrl={gitUrl}
                onGitUrlChange={setGitUrl}
                sectionRef={installRef}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
