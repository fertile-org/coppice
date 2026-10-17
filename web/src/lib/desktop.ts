export type DesktopAppInfo = {
  version: string;
  platform: string;
  arch: string;
  /** True only for a signed macOS build that installs updates in place. */
  macInPlaceUpdates: boolean;
};

export type DesktopUpdateInfo =
  | { kind: 'linux'; version: string }
  | { kind: 'download'; version: string; url: string }
  | { kind: 'ready'; version: string };

export type DesktopUpdateSettings = {
  automaticUpdates: boolean;
};

export type DesktopUpdateCheck =
  | { status: 'up-to-date' }
  | { status: 'available'; version: string }
  | { status: 'error' };

export type CoppiceDesktopBridge = {
  pickDirectory: () => Promise<string | null>;
  appInfo?: () => Promise<DesktopAppInfo>;
  getUpdateInfo?: () => Promise<DesktopUpdateInfo | null>;
  onUpdateInfo?: (listener: (info: DesktopUpdateInfo | null) => void) => () => void;
  getUpdateSettings?: () => Promise<DesktopUpdateSettings>;
  setUpdateSettings?: (settings: DesktopUpdateSettings) => Promise<DesktopUpdateSettings>;
  checkForUpdates?: () => Promise<DesktopUpdateCheck>;
  restartToUpdate?: () => Promise<void>;
  /** `process.platform` from the Electron shell (`darwin`, `linux`, …). */
  platform?: string;
  showItemInFolder?: (filePath: string) => Promise<void>;
};

declare global {
  interface Window {
    coppiceDesktop?: CoppiceDesktopBridge;
  }
}

export function isDesktopShell(): boolean {
  return typeof window !== 'undefined' && window.coppiceDesktop != null;
}

export async function pickDirectory(): Promise<string | null> {
  if (!window.coppiceDesktop?.pickDirectory) {
    return null;
  }
  return window.coppiceDesktop.pickDirectory();
}

export function revealFileLabel(platform: string | undefined): string {
  return platform === 'darwin' ? 'Reveal in Finder' : 'Reveal in file manager';
}

export async function revealFileInFolder(filePath: string): Promise<void> {
  await window.coppiceDesktop?.showItemInFolder?.(filePath);
}
