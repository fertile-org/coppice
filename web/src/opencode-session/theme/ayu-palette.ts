/**
 * Ayu Dark tokens for OpenCode session UI and xterm live console.
 * Keep in sync with `opencode-theme.css`.
 */
export const ayuPalette = {
  bg: '#0d1017',
  bgPanel: '#141821',
  bgElement: '#10141c',
  border: '#1b1f29',
  text: '#bfbdb6',
  textMuted: '#555e73',
  textDim: '#626d7a',
  primary: '#e6b450',
  secondary: '#59c2ff',
  success: '#70bf56',
  warning: '#ff8f40',
  error: '#d95757',
  info: '#39bae6',
  thinking: '#e6b450',
  markdownCode: '#aad94c',
} as const;

/** xterm 16-color theme aligned to Ayu / OpenCode semantic colors. */
export const openCodeXtermTheme = {
  background: ayuPalette.bg,
  foreground: ayuPalette.text,
  cursor: ayuPalette.text,
  cursorAccent: ayuPalette.bg,
  selectionBackground: '#1b2738',
  black: ayuPalette.bg,
  red: ayuPalette.error,
  green: ayuPalette.success,
  yellow: ayuPalette.primary,
  blue: ayuPalette.secondary,
  magenta: '#d2a6ff',
  cyan: ayuPalette.info,
  white: ayuPalette.text,
  brightBlack: ayuPalette.textMuted,
  brightRed: ayuPalette.error,
  brightGreen: ayuPalette.markdownCode,
  brightYellow: ayuPalette.warning,
  brightBlue: ayuPalette.secondary,
  brightMagenta: '#f07178',
  brightCyan: ayuPalette.info,
  brightWhite: ayuPalette.text,
} as const;
