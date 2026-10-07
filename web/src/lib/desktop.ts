export type DesktopAppInfo = {
  version: string;
  platform: string;
  arch: string;
};

export type DesktopUpdateInfo = {
  version: string;
  url: string;
};

export type CoppiceDesktopBridge = {
  pickDirectory: () => Promise<string | null>;
  appInfo?: () => Promise<DesktopAppInfo>;
  getUpdateInfo?: () => Promise<DesktopUpdateInfo | null>;
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
