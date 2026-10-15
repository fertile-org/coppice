/** macOS uses ⌘ (`metaKey`). Linux, Windows, and WSL use Ctrl (`ctrlKey`). */
export type Platform = 'macos' | 'linux' | 'windows' | 'wsl';

type PlatformSource = {
  platform?: string;
  userAgent?: string;
  userAgentData?: { platform?: string };
};

function readPlatformSource(source?: PlatformSource | null): PlatformSource | null {
  if (source !== undefined) return source;
  if (typeof navigator === 'undefined') return null;
  const nav = navigator as Navigator & PlatformSource;
  return {
    platform: nav.platform,
    userAgent: nav.userAgent,
    userAgentData: nav.userAgentData,
  };
}

export function detectPlatform(source?: PlatformSource | null): Platform {
  const nav = readPlatformSource(source);
  if (!nav) return 'linux';
  const blob = `${nav.userAgentData?.platform ?? ''} ${nav.platform ?? ''} ${nav.userAgent ?? ''}`;
  if (/Mac/i.test(blob) && !/iPhone|iPad|iPod/i.test(blob)) return 'macos';
  if (/Win/i.test(blob)) return 'windows';
  if (/Linux/i.test(blob) && /Microsoft|WSL/i.test(blob)) return 'wsl';
  return 'linux';
}

export function usesMetaMod(platform: Platform): boolean {
  return platform === 'macos';
}

/** Mod is pressed, and the other modifier (Ctrl on macOS, ⌘ elsewhere) is not. */
export function modPressed(
  event: { metaKey: boolean; ctrlKey: boolean },
  platform: Platform,
): boolean {
  if (usesMetaMod(platform)) return event.metaKey && !event.ctrlKey;
  return event.ctrlKey && !event.metaKey;
}
