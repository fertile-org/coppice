import { useContext, useEffect, useMemo, useRef, useState } from 'react';
import { Dialog, DialogContent, DialogTitle } from '../../components/ui/dialog';
import { SessionContext } from '../auth/useSession';
import { HELP_COPY, helpSections, type HelpSection } from './help';
import { KeyboardContext } from './keyboardContext';
import { eventMatchesBinding } from './match';
import { detectPlatform } from './platform';
import { omittedActionIds, SHORTCUT_ACTIONS } from './registry';
import { useRegisterShortcut } from './useKeyboard';

export function useShortcutHelp() {
  const session = useContext(SessionContext);
  const keyboard = useContext(KeyboardContext);
  const platform = useMemo(() => detectPlatform(), []);
  const actions = keyboard?.actions ?? SHORTCUT_ACTIONS;
  const keybindingWarnings = keyboard?.keybindingWarnings ?? [];
  const sections = helpSections({
    omitted: omittedActionIds({
      isAdmin: session?.user?.role === 'admin',
      desktopMode: session?.desktopMode ?? false,
    }),
    platform,
    actions,
  });
  return { actions, keybindingWarnings, platform, sections };
}

export function ShortcutHelpSections({ sections }: { sections: HelpSection[] }) {
  return (
    <>
      {sections.map((section) => (
        <section key={section.group} className="mt-5">
          <h3 className="font-body text-xs font-medium uppercase tracking-wide text-text-muted">
            {section.group}
          </h3>
          <ul className="mt-2">
            {section.rows.map((row) => (
              <li
                key={row.id}
                className="flex items-baseline justify-between gap-4 border-b border-border/70 py-1.5 last:border-b-0"
              >
                <span className="font-body text-sm text-text-primary">{row.label}</span>
                <span className="shrink-0 font-mono text-xs text-text-muted">
                  {row.shortcut}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}

export function ShortcutHelpFooter({ skipped }: { skipped: boolean }) {
  return (
    <>
      <p className="font-body text-sm text-text-muted">{HELP_COPY.footer}</p>
      {skipped ? (
        <p className="mt-1 font-body text-sm text-text-muted">{HELP_COPY.skipped}</p>
      ) : null}
    </>
  );
}

export function ShortcutHelp() {
  const { actions, keybindingWarnings, platform, sections } = useShortcutHelp();
  const [open, setOpen] = useState(false);
  const restoreFocus = useRef<HTMLElement | null>(null);

  useRegisterShortcut('help.show', () => {
    const active = document.activeElement;
    setOpen((current) => {
      if (current) return false;
      restoreFocus.current = active instanceof HTMLElement ? active : null;
      return true;
    });
  });

  useEffect(() => {
    if (!open) return;
    const help = actions.find((action) => action.id === 'help.show');
    function onKeyDown(event: KeyboardEvent) {
      const matches = help?.bindings.some((binding) =>
        eventMatchesBinding(binding, event, platform),
      );
      if (!matches || event.repeat) return;
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
    document.addEventListener('keydown', onKeyDown, true);
    return () => document.removeEventListener('keydown', onKeyDown, true);
  }, [actions, open, platform]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="z-[70] flex max-h-[min(40rem,85vh)] max-w-lg flex-col overflow-hidden p-0"
        overlayClassName="z-[70]"
        aria-describedby={undefined}
        onCloseAutoFocus={(event) => {
          const node = restoreFocus.current;
          restoreFocus.current = null;
          if (!(node instanceof HTMLElement) || !node.isConnected) return;
          event.preventDefault();
          node.focus();
        }}
      >
        <div className="overflow-y-auto p-6">
          <DialogTitle>{HELP_COPY.title}</DialogTitle>
          <ShortcutHelpSections sections={sections} />
        </div>
        <div className="border-t border-border px-6 py-3">
          <ShortcutHelpFooter skipped={keybindingWarnings.length > 0} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
