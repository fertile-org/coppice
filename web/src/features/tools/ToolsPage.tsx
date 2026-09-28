import { useRef, useState, type FormEvent } from 'react';
import { Navigate } from 'react-router-dom';
import { ApiError, apiFetch, parseApiErrorMessage } from '../../lib/api';
import { useSession } from '../auth/useSession';
import { Button } from '../../components/ui/button';

export function ToolsPage() {
  const { user } = useSession();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [confirmImport, setConfirmImport] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }

  async function handleExport() {
    setExporting(true);
    setError(null);
    setMessage(null);
    try {
      const res = await apiFetch('/api/tools/backup/export');
      const blob = await res.blob();
      const disposition = res.headers.get('Content-Disposition') ?? '';
      const match = /filename="([^"]+)"/.exec(disposition);
      const filename = match?.[1] ?? 'coppice-backup.zip';
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename;
      anchor.click();
      URL.revokeObjectURL(url);
      setMessage('Export downloaded. Store the archive securely — it contains secrets and all workspace data.');
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Export failed.'));
    } finally {
      setExporting(false);
    }
  }

  async function handleImport(e: FormEvent) {
    e.preventDefault();
    const file = fileInputRef.current?.files?.[0];
    if (!file) {
      setError('Choose a backup .zip file first.');
      return;
    }
    if (!confirmImport) {
      setError('Confirm that you want to replace all data before importing.');
      return;
    }

    setImporting(true);
    setError(null);
    setMessage(null);
    try {
      const form = new FormData();
      form.append('file', file);
      form.append('confirm', 'REPLACE_ALL');
      const res = await apiFetch('/api/tools/backup/import', {
        method: 'POST',
        body: form,
      });
      const data = (await res.json()) as { message?: string };
      setMessage(data.message ?? 'Import completed.');
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
      setConfirmImport(false);
    } catch (err) {
      if (err instanceof ApiError && err.status === 503) {
        setError(
          'Import needs pg_dump/psql on the server host (bundled with Postgres in the future desktop app).',
        );
      } else {
        setError(parseApiErrorMessage(err, 'Import failed.'));
      }
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="space-y-8">
      <header className="border-b border-border pb-6">
        <h1 className="font-display text-2xl font-semibold text-text-primary">
          Tools
        </h1>
        <p className="mt-2 max-w-2xl font-body text-sm text-text-secondary">
          Export or import a full Coppice archive: PostgreSQL dump, runtime
          config snapshot, uploaded artifacts, and agent worktrees. Intended for
          migration and desktop backups — not for sharing publicly.
        </p>
      </header>

      {message && (
        <p
          className="rounded-lg border border-moss-600/30 bg-accent-muted px-4 py-3 font-body text-sm text-text-primary"
          role="status"
        >
          {message}
        </p>
      )}
      {error && (
        <p
          className="rounded-lg border border-danger/30 bg-danger-muted px-4 py-3 font-body text-sm text-danger"
          role="alert"
        >
          {error}
        </p>
      )}

      <section className="rounded-xl border border-border bg-surface-raised p-6 shadow-card">
        <h2 className="font-display text-lg font-semibold text-text-primary">
          Export backup
        </h2>
        <p className="mt-2 font-body text-sm text-text-secondary">
          Downloads a zip file. Requires{' '}
          <code className="font-mono text-xs">pg_dump</code> on the server machine.
        </p>
        <Button
          type="button"
          className="mt-4"
          disabled={exporting}
          onClick={() => void handleExport()}
        >
          {exporting ? 'Preparing export…' : 'Download backup'}
        </Button>
      </section>

      <section className="rounded-xl border border-border bg-surface-raised p-6 shadow-card">
        <h2 className="font-display text-lg font-semibold text-text-primary">
          Import backup
        </h2>
        <p className="mt-2 font-body text-sm text-text-secondary">
          Replaces the current database and on-disk Coppice data directories with
          the archive contents. Active sessions may be invalidated.
        </p>
        <form className="mt-4 space-y-4" onSubmit={(e) => void handleImport(e)}>
          <div>
            <label
              htmlFor="backup-file"
              className="mb-1 block font-body text-sm font-medium text-bark-800"
            >
              Backup file (.zip)
            </label>
            <input
              id="backup-file"
              ref={fileInputRef}
              type="file"
              accept=".zip,application/zip"
              className="block w-full max-w-md font-body text-sm"
            />
          </div>
          <label className="flex items-start gap-2 font-body text-sm text-text-secondary">
            <input
              type="checkbox"
              checked={confirmImport}
              onChange={(e) => setConfirmImport(e.target.checked)}
              className="mt-1"
            />
            <span>
              I understand this will <strong className="text-text-primary">replace all</strong>{' '}
              Coppice data in this instance with the backup.
            </span>
          </label>
          <Button type="submit" variant="destructive" disabled={importing}>
            {importing ? 'Importing…' : 'Import backup'}
          </Button>
        </form>
      </section>
    </div>
  );
}
