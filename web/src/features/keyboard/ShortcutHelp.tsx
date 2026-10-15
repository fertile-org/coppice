import { useContext, useEffect, useMemo, useRef, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '../../components/ui/dialog';
import { SessionContext } from '../auth/useSession';
import { HELP_COPY, helpSections } from './help';
import { detectPlatform } from './platform';
import { omittedActionIds } from './registry';
import { useRegisterShortcut } from './useKeyboard';

export function ShortcutHelp() {
  const session = useContext(SessionContext);
  const platform = useMemo(() => detectPlatform(), []);
  const [open, setOpen] = useState(false);
  const restoreFocus = useRef<HTMLElement | null>(null);
  const sections = helpSections({
    omitted: omittedActionIds({
      isAdmin: session?.user?.role === 'admin',
      desktopMode: session?.desktopMode ?? false,
    }),
    platform,
  });

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
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== '?' || event.metaKey || event.ctrlKey || event.altKey || event.repeat) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
    document.addEventListener('keydown', onKeyDown, true);
    return () => document.removeEventListener('keydown', onKeyDown, true);
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="z-[70] flex max-h-[min(40rem,85vh)] max-w-lg flex-col overflow-hidden p-0"
        overlayClassName="z-[70]"
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
          <DialogDescription className="mt-5">{HELP_COPY.footer}</DialogDescription>
        </div>
      </DialogContent>
    </Dialog>
  );
}
