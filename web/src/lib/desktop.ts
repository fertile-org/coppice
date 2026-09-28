export type CoppiceDesktopBridge = {
  pickDirectory: () => Promise<string | null>;
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
