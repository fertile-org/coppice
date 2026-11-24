export const TEXT_SIZE_STORAGE_KEY = 'coppice.textSize';

/** Body text size in CSS px (`text-sm` after root scale). */
export type TextSize = 14 | 15 | 16 | 17;

export const TEXT_SIZES: TextSize[] = [14, 15, 16, 17];

export const TEXT_SIZE_LABELS: Record<TextSize, string> = {
  14: 'Small',
  15: 'Default',
  16: 'Large',
  17: 'Larger',
};

export const DEFAULT_TEXT_SIZE: TextSize = 15;

/** Browser default root (16px) × N/14 so `text-sm` (0.875rem) renders at N px. */
export function rootFontSizeCss(size: TextSize): string {
  return `calc(16px * ${size} / 14)`;
}

export function isTextSize(value: string | null): value is `${TextSize}` {
  return value === '14' || value === '15' || value === '16' || value === '17';
}

export function parseTextSize(value: string | null): TextSize {
  if (isTextSize(value)) {
    return Number(value) as TextSize;
  }
  return DEFAULT_TEXT_SIZE;
}

export function readStoredTextSize(): TextSize {
  try {
    return parseTextSize(localStorage.getItem(TEXT_SIZE_STORAGE_KEY));
  } catch {
    /* ignore quota / private mode */
  }
  return DEFAULT_TEXT_SIZE;
}

export function applyTextSize(size: TextSize): void {
  document.documentElement.style.fontSize = rootFontSizeCss(size);
}

export function storeTextSize(size: TextSize): void {
  try {
    localStorage.setItem(TEXT_SIZE_STORAGE_KEY, String(size));
  } catch {
    /* ignore quota / private mode */
  }
}
