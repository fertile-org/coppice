import { ChevronLeft, ChevronRight } from 'lucide-react';
import type * as React from 'react';
import { DayPicker } from 'react-day-picker';
import { cn } from '../../lib/utils';

function Calendar({ className, classNames, ...props }: React.ComponentProps<typeof DayPicker>) {
  return (
    <DayPicker
      className={cn('font-body text-sm', className)}
      classNames={{
        months: 'relative flex flex-col',
        month: 'space-y-3',
        month_caption: 'flex h-8 items-center justify-center font-medium text-text-primary',
        nav: 'absolute inset-x-0 top-0 flex h-8 items-center justify-between',
        button_previous:
          'inline-flex size-7 items-center justify-center rounded-md text-text-secondary hover:bg-paper-100 hover:text-text-primary',
        button_next:
          'inline-flex size-7 items-center justify-center rounded-md text-text-secondary hover:bg-paper-100 hover:text-text-primary',
        month_grid: 'w-full border-collapse',
        weekdays: 'flex',
        weekday: 'w-9 text-center text-xs font-normal text-text-muted',
        week: 'mt-1 flex',
        day: 'size-9 p-0 text-center',
        day_button:
          'inline-flex size-9 items-center justify-center rounded-md text-text-primary hover:bg-paper-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted',
        selected: '[&>button]:bg-accent [&>button]:text-paper-50 [&>button]:hover:bg-accent',
        today: '[&>button]:font-semibold [&>button]:text-accent',
        outside: '[&>button]:text-text-muted [&>button]:opacity-60',
        disabled: '[&>button]:opacity-40',
        hidden: 'invisible',
        ...classNames,
      }}
      components={{
        Chevron: ({ orientation }) =>
          orientation === 'left' ? (
            <ChevronLeft className="size-4" aria-hidden />
          ) : (
            <ChevronRight className="size-4" aria-hidden />
          ),
      }}
      {...props}
    />
  );
}

export { Calendar };
