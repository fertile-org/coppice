import type * as React from 'react';
import { cn } from '../../lib/utils';

function Textarea({ className, ...props }: React.ComponentProps<'textarea'>) {
  return (
    <textarea
      className={cn(
        'field-control min-h-[80px] w-full resize-y px-3 py-2 font-body text-sm',
        className,
      )}
      {...props}
    />
  );
}

export { Textarea };
