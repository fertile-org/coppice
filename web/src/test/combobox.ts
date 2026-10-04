import { fireEvent, within } from '@testing-library/react';

type Matcher = string | RegExp;

/** Opens a `Combobox` trigger and returns the listbox popover. */
export function openCombobox(trigger: HTMLElement): HTMLElement {
  if (trigger.getAttribute('aria-expanded') !== 'true') {
    fireEvent.click(trigger);
  }
  const contentId = trigger.getAttribute('aria-controls');
  const content = contentId ? document.getElementById(contentId) : null;
  const list = content?.querySelector<HTMLElement>('[cmdk-list]');
  if (!list) throw new Error('Combobox listbox did not open');
  return list;
}

/** Opens a `Combobox` and clicks the option whose accessible name matches `option`. */
export function selectComboboxOption(trigger: HTMLElement, option: Matcher): void {
  const list = openCombobox(trigger);
  fireEvent.click(within(list).getByRole('option', { name: option }));
}

/** Types into the search input of an open `Combobox`. */
export function searchCombobox(text: string): void {
  const input = document.querySelector<HTMLInputElement>('[cmdk-input]');
  if (!input) throw new Error('Combobox is not open or has no search input (searchable is off)');
  fireEvent.change(input, { target: { value: text } });
}
