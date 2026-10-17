const KEY = 'coppice.initWizard';

export type InitWizardFlag = 'skipped' | 'done';

export function initWizardFlag(): InitWizardFlag | null {
  try {
    const value = localStorage.getItem(KEY);
    if (value === 'skipped' || value === 'done') return value;
    return null;
  } catch {
    return null;
  }
}

export function setInitWizardFlag(flag: InitWizardFlag): void {
  try {
    localStorage.setItem(KEY, flag);
  } catch {
    /* private mode or quota */
  }
}
