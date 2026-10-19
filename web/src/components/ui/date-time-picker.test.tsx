import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DateTimePicker } from './date-time-picker';

describe('DateTimePicker', () => {
  it('shows the placeholder and keeps the time when a day is picked', () => {
    const onChange = vi.fn();
    render(<DateTimePicker id="from" value="2026-10-05T09:30" onChange={onChange} />);

    fireEvent.click(screen.getByRole('button', { name: /Oct 5, 2026/ }));
    fireEvent.click(screen.getByRole('button', { name: /October 12/ }));
    expect(onChange).toHaveBeenLastCalledWith('2026-10-12T09:30');

    fireEvent.change(screen.getByLabelText('Time'), { target: { value: '17:05' } });
    expect(onChange).toHaveBeenLastCalledWith('2026-10-05T17:05');
  });

  it('clears the value', () => {
    const onChange = vi.fn();
    render(<DateTimePicker value="2026-10-05T09:30" onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Clear time' }));
    expect(onChange).toHaveBeenCalledWith('');
  });

  it('reads Any time when empty', () => {
    render(<DateTimePicker value="" onChange={() => {}} />);
    expect(screen.getByRole('button', { name: 'Any time' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear time' })).toBeNull();
  });
});
