import { afterEach, describe, expect, it } from 'vitest';
import {
  DEFAULT_TEXT_SIZE,
  TEXT_SIZE_STORAGE_KEY,
  applyTextSize,
  isTextSize,
  parseTextSize,
  readStoredTextSize,
  rootFontSizeCss,
  storeTextSize,
} from './textSize';

describe('textSize helpers', () => {
  afterEach(() => {
    localStorage.clear();
    document.documentElement.style.fontSize = '';
  });

  it('validates size values', () => {
    expect(isTextSize('14')).toBe(true);
    expect(isTextSize('15')).toBe(true);
    expect(isTextSize('16')).toBe(true);
    expect(isTextSize('17')).toBe(true);
    expect(isTextSize('13')).toBe(false);
    expect(isTextSize('18')).toBe(false);
    expect(isTextSize(null)).toBe(false);
  });

  it('parses stored values and falls back to Default', () => {
    expect(parseTextSize(null)).toBe(DEFAULT_TEXT_SIZE);
    expect(parseTextSize('15')).toBe(15);
    expect(parseTextSize('nope')).toBe(DEFAULT_TEXT_SIZE);
    expect(parseTextSize('13')).toBe(DEFAULT_TEXT_SIZE);
  });

  it('reads stored size and falls back to Default when missing or invalid', () => {
    expect(readStoredTextSize()).toBe(DEFAULT_TEXT_SIZE);
    localStorage.setItem(TEXT_SIZE_STORAGE_KEY, '14');
    expect(readStoredTextSize()).toBe(14);
    localStorage.setItem(TEXT_SIZE_STORAGE_KEY, '17');
    expect(readStoredTextSize()).toBe(17);
    localStorage.setItem(TEXT_SIZE_STORAGE_KEY, 'bogus');
    expect(readStoredTextSize()).toBe(DEFAULT_TEXT_SIZE);
  });

  it('applies root font-size so text-sm equals the chosen px', () => {
    expect(rootFontSizeCss(15)).toBe('calc(16px * 15 / 14)');
    applyTextSize(15);
    // jsdom serializes the calc; browsers keep the expression.
    expect(document.documentElement.style.fontSize).toMatch(/17\.14|16px \* 15/);
    applyTextSize(14);
    expect(document.documentElement.style.fontSize).toMatch(/16px|16px \* 14/);
  });

  it('stores the preference under coppice.textSize', () => {
    storeTextSize(16);
    expect(localStorage.getItem(TEXT_SIZE_STORAGE_KEY)).toBe('16');
  });
});
