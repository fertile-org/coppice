const TEXT_INPUT_TYPES = new Set([
  '',
  'text',
  'search',
  'email',
  'password',
  'url',
  'tel',
  'number',
]);

export type TargetKind = 'terminal' | 'text' | 'plain';

export function classifyTarget(target: EventTarget | null): TargetKind {
  if (!(target instanceof Element)) return 'plain';
  if (target.closest('.xterm')) return 'terminal';
  if (isTextField(target)) return 'text';
  return 'plain';
}

export function isTextField(target: Element): boolean {
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.has(target.type);
  }
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return Boolean(target.closest('.ProseMirror, .cm-editor, .cm-content'));
}

/** Dialogs other than the ticket drawer keep their own keys, including Esc. */
export function isForeignDialog(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  const dialog = target.closest('[role="dialog"]');
  if (!dialog) return false;
  return dialog.getAttribute('data-shortcut-layer') !== 'ticket';
}
