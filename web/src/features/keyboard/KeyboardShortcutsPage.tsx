import { HELP_COPY } from './help';
import { ShortcutHelpFooter, ShortcutHelpSections, useShortcutHelp } from './ShortcutHelp';

export function KeyboardShortcutsPage() {
  const { keybindingWarnings, sections } = useShortcutHelp();

  return (
    <div className="max-w-xl">
      <h1 className="font-display text-2xl font-semibold text-text-primary">{HELP_COPY.title}</h1>
      <p className="mt-2 font-body text-text-secondary">{HELP_COPY.pageIntro}</p>
      <ShortcutHelpSections sections={sections} />
      <div className="mt-8 border-t border-border pt-3">
        <ShortcutHelpFooter skipped={keybindingWarnings.length > 0} />
      </div>
    </div>
  );
}
