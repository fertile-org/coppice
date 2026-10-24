import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useId, useMemo, useState } from 'react';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
import { Switch } from '../../components/ui/switch';
import { ApiError, apiFetch, parseApiErrorMessage } from '../../lib/api';
import { cn } from '../../lib/utils';
import { publishKeybindings, readKeybindings } from '../keyboard/keybindings';

import {
  type ConfigUi,
  defaultText,
  listText,
  matches,
  readConfigUi,
  saveResultSchema,
  type Setting,
  SETTINGS_UI_COPY,
  SETTINGS_UI_QUERY_KEY,
  settingTitle,
  shortcutText,
  type TomlValue,
} from './settingsUiModel';

/**
 * Every config.toml setting as a form, like VS Code's settings UI. A key the
 * file doesn't set shows its default; changing it writes that one key, and
 * Reset removes it again. Each change saves on its own.
 */

type RowStatus = { kind: 'saved' | 'error'; text: string };

export function SettingsUi() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: SETTINGS_UI_QUERY_KEY,
    queryFn: readConfigUi,
    refetchInterval: 2000,
  });
  const [search, setSearch] = useState('');
  const [statuses, setStatuses] = useState<Record<string, RowStatus>>({});
  const [saving, setSaving] = useState<string | null>(null);

  const data = query.data;
  const groups = useMemo(() => {
    const shown = (data?.settings ?? []).filter((setting) => matches(setting, search));
    const byGroup = new Map<string, Setting[]>();
    for (const setting of shown) {
      const list = byGroup.get(setting.group) ?? [];
      list.push(setting);
      byGroup.set(setting.group, list);
    }
    return [...byGroup.entries()];
  }, [data, search]);

  async function save(setting: Setting, value: TomlValue | null) {
    if (!data) return;
    setSaving(setting.key);
    try {
      const response = await apiFetch('/api/settings/config/value', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          path: setting.path,
          value,
          baseRevision: data.revision,
        }),
      });
      const saved = saveResultSchema.parse(await response.json());
      queryClient.setQueryData<ConfigUi>(SETTINGS_UI_QUERY_KEY, (current) =>
        current
          ? {
              ...current,
              revision: saved.revision,
              settings: current.settings.map((item) =>
                item.key === setting.key ? { ...item, value } : item,
              ),
            }
          : current,
      );
      setStatuses((current) => ({
        ...current,
        [setting.key]: {
          kind: 'saved',
          text: saved.restartRequired ? SETTINGS_UI_COPY.savedRestart : SETTINGS_UI_COPY.saved,
        },
      }));
      if (setting.kind.type === 'shortcut') {
        readKeybindings()
          .then((next) => publishKeybindings(next.overrides, next.warnings))
          .catch(() => {
            /* Saved; the open app keeps its previous shortcuts until reload. */
          });
      }
    } catch (error) {
      const text =
        error instanceof ApiError && error.status === 409
          ? SETTINGS_UI_COPY.conflict
          : parseApiErrorMessage(error);
      setStatuses((current) => ({
        ...current,
        [setting.key]: { kind: 'error', text },
      }));
    } finally {
      setSaving(null);
      void queryClient.invalidateQueries({ queryKey: SETTINGS_UI_QUERY_KEY });
    }
  }

  if (query.isLoading) {
    return <p className="font-body text-sm text-text-muted">Loading…</p>;
  }
  if (!data) {
    return (
      <p
        role="alert"
        className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
      >
        {parseApiErrorMessage(query.error)}
      </p>
    );
  }

  return (
    <div className="space-y-6">
      {data.fileError && (
        <p
          role="alert"
          className="rounded-md bg-danger-muted px-3 py-2 font-body text-sm text-danger"
        >
          {SETTINGS_UI_COPY.fileError(data.fileError)}
        </p>
      )}
      <Input
        type="search"
        aria-label={SETTINGS_UI_COPY.search}
        placeholder={SETTINGS_UI_COPY.search}
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        className="max-w-md"
      />
      {groups.length === 0 ? (
        <p className="font-body text-sm text-text-muted">{SETTINGS_UI_COPY.noMatches}</p>
      ) : (
        groups.map(([group, settings]) => (
          <section key={group} aria-label={group} className="space-y-1">
            <h2 className="border-b border-border pb-2 font-display text-lg font-semibold text-text-primary">
              {group}
            </h2>
            <ul>
              {settings.map((setting) => (
                <SettingRow
                  key={setting.key}
                  setting={setting}
                  disabled={data.fileError != null || saving === setting.key}
                  status={statuses[setting.key]}
                  onSave={(value) => void save(setting, value)}
                />
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}

function SettingRow({
  setting,
  disabled,
  status,
  onSave,
}: {
  setting: Setting;
  disabled: boolean;
  status: RowStatus | undefined;
  onSave: (value: TomlValue | null) => void;
}) {
  const id = useId();
  const modified = setting.value != null;
  return (
    <li
      data-testid={`setting-${setting.key}`}
      className={cn('border-l-2 py-3 pl-3', modified ? 'border-accent' : 'border-transparent')}
    >
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <label htmlFor={id} className="font-body text-sm font-medium text-text-primary">
          {settingTitle(setting)}
        </label>
        <code className="font-mono text-xs text-text-muted">{setting.key}</code>
        {modified && (
          <span className="font-body text-xs text-accent">{SETTINGS_UI_COPY.modified}</span>
        )}
      </div>
      {setting.kind.type !== 'shortcut' && (
        <p className="mt-1 font-body text-sm text-text-secondary">
          {setting.description}
          {setting.restart ? ` ${SETTINGS_UI_COPY.restartHint}` : ''}
        </p>
      )}
      <div className="mt-2 flex flex-wrap items-center gap-3">
        <SettingControl id={id} setting={setting} disabled={disabled} onSave={onSave} />
        {modified && (
          <>
            <span className="font-body text-xs text-text-muted">
              {SETTINGS_UI_COPY.defaultIs(defaultText(setting))}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={disabled}
              onClick={() => onSave(null)}
              aria-label={`${SETTINGS_UI_COPY.reset} ${setting.key}`}
            >
              {SETTINGS_UI_COPY.reset}
            </Button>
          </>
        )}
      </div>
      {status && (
        <p
          role={status.kind === 'error' ? 'alert' : 'status'}
          className={cn(
            'mt-1 font-body text-xs',
            status.kind === 'error' ? 'text-danger' : 'text-text-muted',
          )}
        >
          {status.text}
        </p>
      )}
    </li>
  );
}

const UNSET = '__unset__';

function SettingControl({
  id,
  setting,
  disabled,
  onSave,
}: {
  id: string;
  setting: Setting;
  disabled: boolean;
  onSave: (value: TomlValue | null) => void;
}) {
  const current = setting.value ?? setting.default;
  const kind = setting.kind;
  switch (kind.type) {
    case 'bool':
      return (
        <Switch
          id={id}
          checked={current === true}
          disabled={disabled}
          onCheckedChange={(checked) => onSave(checked)}
        />
      );
    case 'optionalBool': {
      const selected = setting.value == null ? UNSET : setting.value ? 'on' : 'off';
      return (
        <Select
          value={selected}
          disabled={disabled}
          onValueChange={(next) => onSave(next === UNSET ? null : next === 'on')}
        >
          <SelectTrigger id={id} className="w-56">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={UNSET}>{setting.defaultLabel ?? kind.unsetLabel}</SelectItem>
            <SelectItem value="on">{SETTINGS_UI_COPY.on}</SelectItem>
            <SelectItem value="off">{SETTINGS_UI_COPY.off}</SelectItem>
          </SelectContent>
        </Select>
      );
    }
    case 'integerChoice':
    case 'choice': {
      const options: (string | number)[] = kind.options;
      return (
        <Select
          value={current == null ? undefined : String(current)}
          disabled={disabled}
          onValueChange={(next) => onSave(kind.type === 'integerChoice' ? Number(next) : next)}
        >
          <SelectTrigger id={id} className="w-56">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {options.map((option) => (
              <SelectItem key={String(option)} value={String(option)}>
                {String(option)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    }
    case 'stringList':
      if (kind.options) {
        return (
          <OptionList
            id={id}
            options={kind.options}
            selected={Array.isArray(current) ? current.map(String) : []}
            disabled={disabled}
            onSave={onSave}
          />
        );
      }
      return (
        <TextField
          id={id}
          value={listText(current)}
          hint={SETTINGS_UI_COPY.listHint}
          disabled={disabled}
          onCommit={(text) =>
            onSave(
              text
                .split(',')
                .map((part) => part.trim())
                .filter(Boolean),
            )
          }
        />
      );
    case 'shortcut':
      return (
        <TextField
          id={id}
          value={shortcutText(current)}
          hint={SETTINGS_UI_COPY.shortcutHint}
          disabled={disabled}
          onCommit={(text) => {
            const keys = text
              .split(/\s+or\s+/i)
              .map((part) => part.trim())
              .filter(Boolean);
            onSave(keys.length === 0 ? '' : keys.length === 1 ? (keys[0] ?? '') : keys);
          }}
        />
      );
    case 'integer':
      return (
        <TextField
          id={id}
          type="number"
          value={current == null ? '' : String(current)}
          min={kind.min ?? undefined}
          max={kind.max ?? undefined}
          disabled={disabled}
          onCommit={(text) => {
            const number = Number(text);
            if (text.trim() !== '' && Number.isInteger(number)) onSave(number);
          }}
        />
      );
    case 'string':
      return (
        <TextField
          id={id}
          value={current == null ? '' : String(current)}
          disabled={disabled}
          onCommit={(text) => onSave(text)}
        />
      );
  }
}

/** Saves on Enter or when focus leaves, and only when the text changed. */
function TextField({
  id,
  value,
  hint,
  type = 'text',
  min,
  max,
  disabled,
  onCommit,
}: {
  id: string;
  value: string;
  hint?: string;
  type?: 'text' | 'number';
  min?: number;
  max?: number;
  disabled: boolean;
  onCommit: (text: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? value;
  function commit() {
    if (draft != null && draft !== value) onCommit(draft);
    setDraft(null);
  }
  return (
    <div className="w-full max-w-md">
      <Input
        id={id}
        type={type}
        value={shown}
        min={min}
        max={max}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            commit();
          } else if (event.key === 'Escape') {
            setDraft(null);
          }
        }}
      />
      {hint && <p className="mt-1 font-body text-xs text-text-muted">{hint}</p>}
    </div>
  );
}

function OptionList({
  id,
  options,
  selected,
  disabled,
  onSave,
}: {
  id: string;
  options: string[];
  selected: string[];
  disabled: boolean;
  onSave: (value: TomlValue) => void;
}) {
  return (
    <div id={id} role="group" className="flex flex-wrap gap-x-4 gap-y-2">
      {options.map((option) => (
        <label
          key={option}
          className="flex items-center gap-1.5 font-body text-sm text-text-primary"
        >
          <input
            type="checkbox"
            checked={selected.includes(option)}
            disabled={disabled}
            onChange={(event) =>
              onSave(
                event.target.checked
                  ? options.filter((item) => item === option || selected.includes(item))
                  : selected.filter((item) => item !== option),
              )
            }
          />
          <code className="font-mono text-xs">{option}</code>
        </label>
      ))}
    </div>
  );
}
