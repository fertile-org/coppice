import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { apiFetch } from '../lib/api';

export const IMPORT_NOTICE =
  'Imported your git author, worker, knowledge and connector settings from `~/.config/coppice/config.toml`. Connectors stay as set in Connectors. The old file is now `config.toml.imported`.';

async function fetchImportNotice(): Promise<string | null> {
  const res = await apiFetch('/api/import-notice');
  const data = (await res.json()) as { message?: unknown };
  return typeof data.message === 'string' && data.message.trim() ? data.message : null;
}

export function ImportNoticeBanner() {
  const [dismissed, setDismissed] = useState(false);
  const notice = useQuery({
    queryKey: ['import-notice'],
    queryFn: fetchImportNotice,
    staleTime: Infinity,
    retry: false,
  });
  const message = notice.data;
  if (dismissed || !message) return null;

  return (
    <div
      role="status"
      data-testid="import-notice"
      className="flex items-start justify-between gap-3 border-b border-border bg-amber-50 px-4 py-2 font-body text-sm text-bark-900"
    >
      <p>{message}</p>
      <button
        type="button"
        onClick={() => setDismissed(true)}
        className="shrink-0 font-medium text-moss-700 underline-offset-2 hover:underline"
      >
        Dismiss
      </button>
    </div>
  );
}
