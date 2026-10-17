import { createContext } from 'react';
import type { ShortcutAction } from './types';

export type ShortcutRegistration = {
  run: () => void | boolean;
  enabled: () => boolean;
};

export type LayerHandlers = {
  closeDrawer?: () => void;
  focusDrawerTabs?: () => boolean;
};

export type KeyboardEnvironment = {
  drawerOpen: boolean;
  modalOpen: boolean;
};

export type KeyboardContextValue = {
  /** Registry actions with `[keybindings]` overrides applied. */
  actions: readonly ShortcutAction[];
  /** Skip warnings from the last applied `[keybindings]` table. */
  keybindingWarnings: readonly string[];
  register: (id: string, registration: ShortcutRegistration) => () => void;
  registerLayer: (layer: LayerHandlers) => () => void;
  setEnvironment: (environment: KeyboardEnvironment) => void;
  /** Enabled shortcut ids, including Close, and whether the ticket drawer is open. */
  readShortcutState: () => { enabledIds: string[]; drawerOpen: boolean };
  /** Runs a registered shortcut. Returns false when nothing handled it. */
  runShortcut: (id: string) => boolean;
};

export const KeyboardContext = createContext<KeyboardContextValue | null>(null);
