import { useState, type FormEvent } from 'react';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { useConfirm } from '../../components/ui/confirm-dialog';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import { parseApiErrorMessage } from '../../lib/api';
import type { MarketplaceSource } from '../../lib/schemas/marketplace';
import { PLUGIN_COPY } from './copy';
import { isCoppiceSource, trustLabel } from './marketplaceTrust';
import {
  useAddMarketplaceSource,
  useRefreshMarketplaceSource,
  useRemoveMarketplaceSource,
  type RefreshMarketplaceOnce,
} from './useMarketplace';

const ERROR_CLASS =
  'rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger';

interface MarketplaceSourcesDialogProps {
  sources: MarketplaceSource[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  refreshOnce: RefreshMarketplaceOnce;
}

export function MarketplaceSourcesDialog({
  sources,
  open,
  onOpenChange,
  refreshOnce,
}: MarketplaceSourcesDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] max-w-4xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Marketplace sources</DialogTitle>
          <DialogDescription>{PLUGIN_COPY.sourcesIntro}</DialogDescription>
        </DialogHeader>
        <MarketplaceSources sources={sources} refreshOnce={refreshOnce} />
      </DialogContent>
    </Dialog>
  );
}

function MarketplaceSources({
  sources,
  refreshOnce,
}: {
  sources: MarketplaceSource[];
  refreshOnce: RefreshMarketplaceOnce;
}) {
  const [gitUrl, setGitUrl] = useState('');
  const [name, setName] = useState('');
  const [ref, setRef] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const addSource = useAddMarketplaceSource();
  const refreshSource = useRefreshMarketplaceSource();
  const removeSource = useRemoveMarketplaceSource();
  const confirm = useConfirm();
  const busy = refreshSource.isPending || removeSource.isPending;

  async function handleAdd(e: FormEvent) {
    e.preventDefault();
    const trimmedName = name.trim();
    const trimmedUrl = gitUrl.trim();
    if (!trimmedName || !trimmedUrl) {
      setError('Name and git URL are required.');
      return;
    }

    setAdding(true);
    setError(null);
    let added: MarketplaceSource;
    try {
      added = await addSource.mutateAsync({
        name: trimmedName,
        gitUrl: trimmedUrl,
        ref: ref.trim() || undefined,
      });
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to add marketplace.'));
      setAdding(false);
      return;
    }

    setName('');
    setGitUrl('');
    setRef('');
    try {
      await refreshOnce(added);
    } catch (err) {
      setError(
        `Added ${added.name}, but its catalog could not be fetched. ${parseApiErrorMessage(err, 'Use Retry to try again.')}`,
      );
    } finally {
      setAdding(false);
    }
  }

  async function handleRefresh(source: MarketplaceSource) {
    setError(null);
    try {
      await refreshSource.mutateAsync(source.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to refresh marketplace.'));
    }
  }

  async function handleRemove(source: MarketplaceSource) {
    if (
      !(await confirm({
        title: `Remove marketplace "${source.name}"?`,
        description: 'Plugins installed from it stay installed, but lose their Update button.',
        danger: true,
      }))
    ) {
      return;
    }
    setError(null);
    try {
      await removeSource.mutateAsync(source.id);
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to remove marketplace.'));
    }
  }

  return (
    <section className="space-y-3">
      <ol className="mt-4 overflow-hidden rounded-xl border border-border bg-surface-raised shadow-card">
        {sources.map((source) => {
          const coppice = isCoppiceSource(source);
          return (
            <li
              key={source.id}
              data-testid={`marketplace-source-${source.id}`}
              className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-3 last:border-b-0"
            >
              <span className="font-body text-sm font-medium text-text-primary">{source.name}</span>
              <Badge variant="outline">{trustLabel(source)}</Badge>
              <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-secondary">
                {source.gitUrl ?? ''}
              </span>
              <span className="font-body text-xs text-text-muted">
                {source.lastError
                  ? `Last refresh failed: ${source.lastError}`
                  : source.refreshedAt
                    ? `Refreshed ${new Date(source.refreshedAt).toLocaleString()}`
                    : 'Not fetched yet'}
              </span>
              {!coppice && (
                <div className="flex items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-label={
                      source.lastError ? `Retry ${source.name}` : `Refresh ${source.name}`
                    }
                    disabled={busy}
                    onClick={() => void handleRefresh(source)}
                  >
                    {source.lastError ? 'Retry' : 'Refresh'}
                  </Button>
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    aria-label={`Remove ${source.name}`}
                    disabled={busy}
                    onClick={() => void handleRemove(source)}
                  >
                    Remove
                  </Button>
                </div>
              )}
            </li>
          );
        })}
      </ol>

      <form
        onSubmit={(e) => void handleAdd(e)}
        className="grid gap-3 sm:grid-cols-[1fr_2fr_1fr_auto] sm:items-end"
      >
        <div>
          <label
            htmlFor="marketplace-source-name"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Name
          </label>
          <input
            id="marketplace-source-name"
            type="text"
            autoComplete="off"
            placeholder="acme-plugins"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="field-control w-full px-3 py-2 font-body text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="marketplace-source-url"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Git URL
          </label>
          <input
            id="marketplace-source-url"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="https://github.com/org/plugins.git"
            value={gitUrl}
            onChange={(e) => setGitUrl(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <div>
          <label
            htmlFor="marketplace-source-ref"
            className="mb-1 block font-body text-sm font-medium text-bark-800"
          >
            Ref <span className="font-normal text-text-muted">(optional)</span>
          </label>
          <input
            id="marketplace-source-ref"
            type="text"
            autoComplete="off"
            spellCheck={false}
            placeholder="main"
            value={ref}
            onChange={(e) => setRef(e.target.value)}
            className="field-control w-full px-3 py-2 font-mono text-sm"
          />
        </div>
        <Button type="submit" loading={adding}>
          Add marketplace
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
