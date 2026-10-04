import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { openCombobox, searchCombobox, selectComboboxOption } from '../../test/combobox';
import { Combobox, type ComboboxOption } from './combobox';
import { Label } from './label';

const fruits: ComboboxOption[] = [
  { value: 'apple', label: 'Apple' },
  { value: 'banana', label: 'Banana', description: 'Yellow and curved' },
  { value: 'cherry', label: 'Cherry' },
  { value: 'date', label: 'Date', disabled: true },
  { value: 'elder', label: 'Elderberry' },
  { value: 'fig', label: 'Fig' },
];

function Harness({
  onChange,
  initial = '',
  ...rest
}: {
  onChange?: (v: string) => void;
  initial?: string;
} & Partial<React.ComponentProps<typeof Combobox>>) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <Label htmlFor="fruit">Fruit</Label>
      <Combobox
        id="fruit"
        value={value}
        onValueChange={(v) => {
          setValue(v);
          onChange?.(v);
        }}
        options={fruits}
        placeholder="Pick a fruit"
        {...rest}
      />
    </>
  );
}

describe('Combobox', () => {
  it('is labelled via htmlFor and shows the placeholder', () => {
    render(<Harness />);
    const trigger = screen.getByLabelText('Fruit');
    expect(trigger).toHaveAttribute('role', 'combobox');
    expect(trigger).toHaveTextContent('Pick a fruit');
  });

  it('selects an option and calls onValueChange', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    const trigger = screen.getByLabelText('Fruit');
    selectComboboxOption(trigger, 'Cherry');
    expect(onChange).toHaveBeenCalledWith('cherry');
    expect(trigger).toHaveTextContent('Cherry');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  it('filters options by label, value and description', () => {
    render(<Harness />);
    const list = openCombobox(screen.getByLabelText('Fruit'));
    searchCombobox('yellow');
    expect(within(list).getAllByRole('option')).toHaveLength(1);
    expect(within(list).getByRole('option', { name: /Banana/ })).toBeInTheDocument();

    searchCombobox('zzz');
    expect(within(list).queryAllByRole('option')).toHaveLength(0);
    expect(screen.getByText('No results found.')).toBeInTheDocument();
  });

  it('marks the selected option and does not select disabled options', () => {
    const onChange = vi.fn();
    render(<Harness initial="banana" onChange={onChange} />);
    const list = openCombobox(screen.getByLabelText('Fruit'));
    expect(within(list).getByRole('option', { name: /Banana/ })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    fireEvent.click(within(list).getByRole('option', { name: 'Date' }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('supports a clearable empty option', () => {
    const onChange = vi.fn();
    render(<Harness initial="apple" onChange={onChange} clearable clearLabel="None" />);
    const trigger = screen.getByLabelText('Fruit');
    selectComboboxOption(trigger, 'None');
    expect(onChange).toHaveBeenCalledWith('');
    expect(trigger).toHaveTextContent('None');
  });

  it('hides the search input for short lists', () => {
    render(<Harness options={fruits.slice(0, 3)} />);
    openCombobox(screen.getByLabelText('Fruit'));
    expect(document.querySelector('[cmdk-input]')).toBeNull();
  });

  it('supports keyboard selection without a search input', () => {
    const onChange = vi.fn();
    render(<Harness options={fruits.slice(0, 3)} onChange={onChange} />);
    openCombobox(screen.getByLabelText('Fruit'));
    const root = document.querySelector<HTMLElement>('[cmdk-root]')!;
    expect(root).toHaveFocus();
    fireEvent.keyDown(root, { key: 'ArrowDown' });
    fireEvent.keyDown(root, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('banana');
  });

  it('selects with the keyboard', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    openCombobox(screen.getByLabelText('Fruit'));
    const input = document.querySelector<HTMLInputElement>('[cmdk-input]')!;
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('banana');
  });
});
