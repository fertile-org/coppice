import { useRef, useState, type FormEvent } from 'react';
import { ApiError, apiFetch, parseApiErrorMessage } from '../../lib/api';
import { MIN_PASSWORD_CHARS } from '../../lib/passwords';
import { Button } from '../../components/ui/button';
import { useSession, type User } from '../auth/useSession';

export function BackupTab() {
  const { desktopMode, establishSession } = useSession();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [replacing, setReplacing] = useState(false);
  const [confirmImport, setConfirmImport] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState(false);
  const [adminPassword, setAdminPassword] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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
      setMessage(
        'Backup downloaded. It holds all your workspace data, so keep it private.',
      );
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
      setMessage(data.message ?? 'Import finished.');
      if (fileInputRef.current) {
        fileInputRef.current.value = '';
      }
      setConfirmImport(false);
    } catch (err) {
      if (err instanceof ApiError && err.status === 503) {
        setError(
          "The desktop app has everything it needs to import. A server install needs PostgreSQL's `pg_dump` and `psql`.",
        );
      } else {
        setError(parseApiErrorMessage(err, 'Import failed.'));
      }
    } finally {
      setImporting(false);
    }
  }

  async function handleReplace() {
    if (!confirmReplace) {
      setError('Check the box to continue.');
      setMessage(null);
      return;
    }
    if (adminPassword.trim() === '') {
      setError('Enter a new admin password.');
      setMessage(null);
      return;
    }
    if ([...adminPassword].length < MIN_PASSWORD_CHARS) {
      setError(`Use at least ${MIN_PASSWORD_CHARS} characters.`);
      setMessage(null);
      return;
    }
    setReplacing(true);
    setError(null);
    setMessage(null);
    try {
      await apiFetch('/api/tools/keys/replace', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ adminPassword }),
      });
      setAdminPassword('');
      setConfirmReplace(false);
      try {
        const renew = await apiFetch('/api/auth/desktop-session', { method: 'POST' });
        const session = (await renew.json()) as { user: User; csrfToken: string };
        establishSession(session.user, session.csrfToken);
        setMessage('Keys replaced. Saved tokens still work.');
      } catch {
        setMessage('Keys replaced. Reload the page to sign in again.');
      }
    } catch (err) {
      setError(parseApiErrorMessage(err, "Couldn't replace keys. Nothing was changed."));
    } finally {
      setReplacing(false);
    }
  }

  return (
    <div className="space-y-8">
      <p className="max-w-2xl font-body text-sm text-text-secondary">
        Download a backup of your boards, tickets, settings, files, and agent
        worktrees. Passwords, keys, and saved tokens are left out. Keep the file
        private.
      </p>

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
          Importing replaces all data on this install with the backup's. Its
          settings are restored too, except passwords, keys, and folder paths.
          You'll paste saved tokens again, and you may be signed out.
        </p>
        <p className="mt-2 font-body text-sm text-text-secondary">
          The desktop app has everything it needs to import. A server install
          needs PostgreSQL's `pg_dump` and `psql`.
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

      {!desktopMode && (
        <p className="max-w-2xl font-body text-sm text-text-secondary">
          On a server, stop Coppice and run `coppice-server replace-keys`.
        </p>
      )}

      {desktopMode && (
        <section className="rounded-xl border border-border bg-surface-raised p-6 shadow-card">
          <h2 className="font-display text-lg font-semibold text-text-primary">
            Replace keys
          </h2>
          <p className="mt-2 font-body text-sm text-text-secondary">
            If you shared a backup, make new keys and a new admin password. Saved
            tokens keep working, and everyone is signed out.
          </p>
          <div className="mt-4 space-y-4">
            <div>
              <label
                htmlFor="new-admin-password"
                className="mb-1 block font-body text-sm font-medium text-bark-800"
              >
                New admin password
              </label>
              <input
                id="new-admin-password"
                type="password"
                autoComplete="new-password"
                value={adminPassword}
                onChange={(e) => setAdminPassword(e.target.value)}
                className="field-control w-full max-w-md px-3 py-2 font-body text-sm"
              />
            </div>
            <label className="flex items-start gap-2 font-body text-sm text-text-secondary">
              <input
                type="checkbox"
                checked={confirmReplace}
                onChange={(e) => setConfirmReplace(e.target.checked)}
                className="mt-1"
              />
              <span>
                I understand everyone will be signed out, and I'll sign in with the
                new password.
              </span>
            </label>
            <Button
              type="button"
              disabled={replacing}
              onClick={() => void handleReplace()}
            >
              {replacing ? 'Replacing keys…' : 'Replace keys'}
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}
