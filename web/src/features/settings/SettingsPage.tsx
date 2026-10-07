import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Navigate } from 'react-router-dom';
import { z } from 'zod';
import { Button } from '../../components/ui/button';
import { ApiError, apiFetch, parseApiErrorMessage } from '../../lib/api';
import { isDesktopShell, revealFileInFolder, revealFileLabel } from '../../lib/desktop';
import { useSession } from '../auth/useSession';
import { TomlEditor } from './TomlEditor';

const SAVED = 'Saved.';
const SAVED_RESTART = 'Saved. Restart Coppice to apply.';

const ERROR_CLASS =
  'rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger';

const configFileSchema = z.object({
  path: z.string(),
  text: z.string(),
  revision: z.string(),
  backupAvailable: z.boolean(),
});

const saveResultSchema = z.object({
  revision: z.string(),
  restartRequired: z.boolean(),
  backupAvailable: z.boolean(),
});

const invalidSchema = z.object({
  line: z.number().int(),
  message: z.string(),
});

const conflictSchema = z.object({
  text: z.string(),
  revision: z.string(),
});

const backupSchema = z.object({
  text: z.string(),
});

type ConfigFile = z.infer<typeof configFileSchema>;

const CONFIG_QUERY_KEY = ['settings-config'] as const;

function invalidSaveMessage(line: number, message: string): string {
  return `Not saved. Line ${line}: ${message}. Your file is unchanged.`;
}

async function readConfig(): Promise<ConfigFile> {
  const response = await apiFetch('/api/settings/config');
  return configFileSchema.parse(await response.json());
}

function jsonBody(error: ApiError): unknown {
  try {
    return JSON.parse(error.body);
  } catch {
    return null;
  }
}

export function SettingsPage() {
  const { user, loading } = useSession();
  const [desktop] = useState(isDesktopShell);
  const queryClient = useQueryClient();
  const configQuery = useQuery({
    queryKey: CONFIG_QUERY_KEY,
    queryFn: readConfig,
    refetchInterval: 2000,
    enabled: user?.role === 'admin',
  });

  const [editor, setEditor] = useState<{
    text: string;
    baseRevision: string;
    staleRevision: string | null;
  } | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [errorLine, setErrorLine] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [restoring, setRestoring] = useState(false);

  const disk = configQuery.data;
  if (disk && editor === null) {
    setEditor({
      text: disk.text,
      baseRevision: disk.revision,
      staleRevision: null,
    });
  }

  const text = editor?.text ?? disk?.text ?? null;
  const baseRevision = editor?.baseRevision ?? disk?.revision ?? null;
  const backupAvailable = disk?.backupAvailable ?? false;

  if (loading) {
    return <p className="font-body text-sm text-text-muted">Loading session…</p>;
  }

  if (user?.role !== 'admin') {
    return <Navigate to="/boards" replace />;
  }

  const conflict =
    disk != null &&
    baseRevision != null &&
    disk.revision !== baseRevision &&
    disk.revision !== editor?.staleRevision;

  function reloadFromDisk() {
    if (!disk) {
      return;
    }
    setEditor({
      text: disk.text,
      baseRevision: disk.revision,
      staleRevision: null,
    });
    setStatus(null);
    setErrorLine(null);
  }

  function keepEdits() {
    if (!disk || !editor) {
      return;
    }
    setEditor({
      ...editor,
      baseRevision: disk.revision,
      staleRevision: null,
    });
    setStatus(null);
  }

  async function save() {
    if (text == null || baseRevision == null || conflict) {
      return;
    }
    setSaving(true);
    try {
      const response = await apiFetch('/api/settings/config', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text, baseRevision }),
      });
      const saved = saveResultSchema.parse(await response.json());
      setEditor((current) =>
        current
          ? {
              ...current,
              baseRevision: saved.revision,
              staleRevision: current.baseRevision,
            }
          : current,
      );
      setStatus(saved.restartRequired ? SAVED_RESTART : SAVED);
      setErrorLine(null);
      queryClient.setQueryData<ConfigFile>(CONFIG_QUERY_KEY, (current) =>
        current
          ? {
              ...current,
              text,
              revision: saved.revision,
              backupAvailable: saved.backupAvailable,
            }
          : current,
      );
    } catch (error) {
      if (error instanceof ApiError && error.status === 400) {
        const parsed = invalidSchema.safeParse(jsonBody(error));
        if (parsed.success) {
          setStatus(invalidSaveMessage(parsed.data.line, parsed.data.message));
          setErrorLine(parsed.data.line);
          return;
        }
      }
      if (error instanceof ApiError && error.status === 409) {
        const parsed = conflictSchema.safeParse(jsonBody(error));
        if (parsed.success) {
          queryClient.setQueryData<ConfigFile>(CONFIG_QUERY_KEY, (current) =>
            current
              ? { ...current, text: parsed.data.text, revision: parsed.data.revision }
              : current,
          );
          setStatus(null);
          setErrorLine(null);
          return;
        }
      }
      setStatus(parseApiErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  async function restore() {
    setRestoring(true);
    try {
      const response = await apiFetch('/api/settings/config/backup');
      const backup = backupSchema.parse(await response.json());
      setEditor((current) => (current ? { ...current, text: backup.text } : current));
      setStatus(null);
      setErrorLine(null);
    } catch (error) {
      setStatus(parseApiErrorMessage(error));
    } finally {
      setRestoring(false);
    }
  }

  const invalid = status?.startsWith('Not saved.') ?? false;

  return (
    <div className="space-y-6">
      <header className="border-b border-border pb-6">
        <h1 className="font-display text-2xl font-semibold text-text-primary">Settings</h1>
        <p className="mt-2 font-body text-sm text-text-secondary">
          This is your <code className="font-mono text-[0.95em]">config.toml</code>. Comments are
          kept.
        </p>
      </header>

      {text == null && configQuery.isLoading ? (
        <p className="font-body text-sm text-text-muted">Loading…</p>
      ) : text == null && configQuery.isError ? (
        <p role="alert" className={ERROR_CLASS}>
          {parseApiErrorMessage(configQuery.error)}
        </p>
      ) : (
        <>
          {disk && (
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <p
                data-testid="config-path"
                className="break-all font-mono text-xs text-text-secondary"
              >
                {disk.path}
              </p>
              {desktop && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-auto px-1 py-0 text-xs"
                  onClick={() => void revealFileInFolder(disk.path)}
                >
                  {revealFileLabel(window.coppiceDesktop?.platform)}
                </Button>
              )}
            </div>
          )}

          {conflict && (
            <div
              role="status"
              className="flex flex-wrap items-center gap-3 rounded-md border border-border bg-surface px-3 py-2"
            >
              <p className="font-body text-sm text-text-primary">
                <code className="font-mono text-[0.95em]">config.toml</code> changed outside this
                editor.
              </p>
              <Button type="button" variant="secondary" onClick={reloadFromDisk}>
                Reload
              </Button>
              <Button type="button" variant="secondary" onClick={keepEdits}>
                Keep my edits
              </Button>
            </div>
          )}

          <TomlEditor
            value={text ?? ''}
            onChange={(next) => {
              setEditor((current) => (current ? { ...current, text: next } : current));
              setErrorLine(null);
              setStatus(null);
            }}
            errorLine={errorLine}
          />

          {status && (
            <p role="alert" className={invalid ? ERROR_CLASS : 'font-body text-sm text-text-primary'}>
              {status}
            </p>
          )}

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              onClick={() => void save()}
              loading={saving}
              disabled={text == null || conflict}
            >
              Save
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => void restore()}
              loading={restoring}
              disabled={!backupAvailable}
            >
              Restore last good version
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
