import type * as React from 'react';
import { cn } from '../../lib/utils';

function Input({ className, type, ...props }: React.ComponentProps<'input'>) {
  return (
    <input
      type={type}
      className={cn('field-control w-full px-3 py-2 font-body text-sm', className)}
      {...props}
    />
  );
}

export { Input };
