import { useState, type FormEvent } from 'react';
import { Button } from '../../components/ui/button';
import { parseApiErrorMessage } from '../../lib/api';
import type { PluginSetting, PluginSettingSource } from '../../lib/schemas/plugin';
import { useSetPluginSettings } from './usePlugins';

const BADGE = 'rounded-full border px-2 py-0.5 font-body text-xs';

function SourceBadge({ source }: { source: PluginSettingSource }) {
  switch (source) {
    case 'env':
      return (
        <span
          className={`${BADGE} border-info-muted bg-info-muted text-info`}
          title="Not set here; the value comes from the Coppice server's environment"
        >
          From server env
        </span>
      );
    case 'default':
      return (
        <span className={`${BADGE} border-border text-text-muted`}>Default</span>
      );
    case 'missing':
      return (
        <span className={`${BADGE} border-warning-muted bg-warning-muted text-warning`}>
          Missing
        </span>
      );
    case 'setting':
      return null;
  }
}

interface PluginSettingsFormProps {
  pluginId: string;
  settings: PluginSetting[];
}

export function PluginSettingsForm({ pluginId, settings }: PluginSettingsFormProps) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const saveSettings = useSetPluginSettings(pluginId);
  const knownKeys = new Set(settings.map((s) => s.key));
  const typed = Object.fromEntries(
    Object.entries(values).filter(([key, value]) => value !== '' && knownKeys.has(key)),
  );

  async function submit(payload: Record<string, string>) {
    setError(null);
    try {
      await saveSettings.mutateAsync(payload);
      return true;
    } catch (err) {
      setError(parseApiErrorMessage(err, 'Unable to save settings.'));
      return false;
    }
  }

  async function handleSave(e: FormEvent) {
    e.preventDefault();
    if (Object.keys(typed).length === 0) return;
    if (await submit(typed)) setValues({});
  }

  return (
    <form onSubmit={(e) => void handleSave(e)} className="mt-3 border-t border-border pt-3">
      <p className="font-body text-xs font-medium text-text-secondary">Settings</p>
      <ul className="mt-2 space-y-2">
        {settings.map(({ key, configured, source }) => {
          const inputId = `plugin-${pluginId}-setting-${key}`;
          return (
            <li
              key={key}
              data-testid={`plugin-setting-${key}`}
              className="flex flex-wrap items-center gap-2"
            >
              <label htmlFor={inputId} className="w-40 truncate font-mono text-xs text-text-primary">
                {key}
              </label>
              <input
                id={inputId}
                type="password"
                autoComplete="off"
                spellCheck={false}
                placeholder={configured ? '••••••••' : ''}
                value={values[key] ?? ''}
                onChange={(e) => setValues((prev) => ({ ...prev, [key]: e.target.value }))}
                className="field-control min-w-[12rem] flex-1 px-3 py-1.5 font-mono text-xs"
              />
              {configured && (
                <>
                  <span className={`${BADGE} border-success-muted bg-success-muted text-success`}>
                    Configured
                  </span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    aria-label={`Clear ${key}`}
                    disabled={saveSettings.isPending}
                    onClick={() =>
                      void submit({ [key]: '' }).then((ok) => {
                        if (!ok) return;
                        setValues((prev) =>
                          Object.fromEntries(Object.entries(prev).filter(([k]) => k !== key)),
                        );
                      })
                    }
                  >
                    Clear
                  </Button>
                </>
              )}
              {!configured && <SourceBadge source={source} />}
            </li>
          );
        })}
      </ul>
      <div className="mt-2 flex items-center gap-2">
        <Button
          type="submit"
          variant="secondary"
          size="sm"
          loading={saveSettings.isPending}
          disabled={Object.keys(typed).length === 0}
        >
          Save settings
        </Button>
      </div>
      {error && (
        <p role="alert" className="mt-2 font-body text-xs text-danger">
          {error}
        </p>
      )}
    </form>
  );
}
