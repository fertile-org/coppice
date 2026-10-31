import { Search } from 'lucide-react';
import { useContext, useMemo, type MouseEvent } from 'react';
import { KeyboardContext } from './keyboardContext';
import { detectPlatform } from './platform';
import { SHORTCUT_ACTIONS } from './registry';
import { formatBinding } from './types';

export function SidebarSearch({ collapsed }: { collapsed: boolean }) {
  const keyboard = useContext(KeyboardContext);
  const platform = useMemo(() => detectPlatform(), []);
  const actions = keyboard?.actions ?? SHORTCUT_ACTIONS;
  const hint = useMemo(() => {
    const binding = actions.find((action) => action.id === 'palette.open')?.bindings[0];
    return binding ? formatBinding(binding, platform) : undefined;
  }, [actions, platform]);

  function onClick(event: MouseEvent<HTMLButtonElement>) {
    event.currentTarget.focus();
    keyboard?.runShortcut('palette.open');
  }

  if (collapsed) {
    return (
      <div className="shrink-0 px-2 pt-2">
        <button
          type="button"
          onClick={onClick}
          aria-label="Search"
          title={hint ? `Search (${hint})` : 'Search'}
          className="flex w-full items-center justify-center rounded-md px-2 py-2.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:bg-paper-200 hover:text-text-primary"
        >
          <Search className="h-[1.125rem] w-[1.125rem] shrink-0" aria-hidden />
        </button>
      </div>
    );
  }

  return (
    <div className="shrink-0 px-2 pt-2">
      <button
        type="button"
        onClick={onClick}
        className="field-control flex w-full min-w-0 items-center gap-2 px-2.5 py-1.5 text-left font-body text-sm"
      >
        <Search className="h-[1.125rem] w-[1.125rem] shrink-0 text-text-muted" aria-hidden />
        <span className="min-w-0 flex-1 truncate text-text-muted">Search…</span>
        {hint ? <kbd className="shrink-0 font-mono text-xs text-text-muted">{hint}</kbd> : null}
      </button>
    </div>
  );
}
