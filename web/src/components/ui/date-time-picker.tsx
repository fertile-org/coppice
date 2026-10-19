import { CalendarDays, X } from 'lucide-react';
import { useState } from 'react';
import { cn } from '../../lib/utils';
import { Button } from './button';
import { Calendar } from './calendar';
import { Input } from './input';
import { Popover, PopoverContent, PopoverTrigger } from './popover';

/** Value is local time as `YYYY-MM-DDTHH:mm`, the same shape a datetime-local input uses. */
type DateTimePickerProps = {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
};

const pad = (n: number) => String(n).padStart(2, '0');

function parseLocal(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(value);
  if (!match) return null;
  const [, y, mo, d, h, mi] = match;
  return new Date(Number(y), Number(mo) - 1, Number(d), Number(h), Number(mi));
}

function toLocal(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

function label(date: Date): string {
  return date.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function DateTimePicker({
  id,
  value,
  onChange,
  placeholder = 'Any time',
  className,
}: DateTimePickerProps) {
  const [open, setOpen] = useState(false);
  const selected = parseLocal(value);
  const time = selected ? `${pad(selected.getHours())}:${pad(selected.getMinutes())}` : '00:00';

  function pickDay(day: Date | undefined) {
    if (!day) return;
    const [h, m] = time.split(':').map(Number);
    day.setHours(h ?? 0, m ?? 0, 0, 0);
    onChange(toLocal(day));
  }

  function pickTime(next: string) {
    const [h, m] = next.split(':').map(Number);
    const base = selected ?? new Date();
    const date = new Date(base);
    date.setHours(h ?? 0, m ?? 0, 0, 0);
    onChange(toLocal(date));
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <div className={cn('relative', className)}>
        <PopoverTrigger asChild>
          <button
            id={id}
            type="button"
            className={cn(
              'field-control flex w-full items-center gap-2 py-2 pl-3 pr-9 text-left font-body text-sm',
              !selected && 'text-text-muted',
            )}
          >
            <CalendarDays className="size-4 shrink-0 text-text-muted" aria-hidden />
            <span className="truncate">{selected ? label(selected) : placeholder}</span>
          </button>
        </PopoverTrigger>
        {selected ? (
          <button
            type="button"
            onClick={() => onChange('')}
            aria-label="Clear time"
            className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-text-muted hover:text-text-primary"
          >
            <X className="size-3.5" aria-hidden />
          </button>
        ) : null}
      </div>
      <PopoverContent align="start" className="w-auto p-3">
        <Calendar
          mode="single"
          selected={selected ?? undefined}
          defaultMonth={selected ?? undefined}
          onSelect={pickDay}
        />
        <div className="mt-3 flex items-center gap-2 border-t border-border pt-3">
          <label htmlFor={id ? `${id}-time` : undefined} className="font-body text-sm text-text-secondary">
            Time
          </label>
          <Input
            id={id ? `${id}-time` : undefined}
            type="time"
            value={time}
            onChange={(event) => pickTime(event.target.value)}
            className="w-auto"
          />
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="ml-auto"
            onClick={() => setOpen(false)}
          >
            Done
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
