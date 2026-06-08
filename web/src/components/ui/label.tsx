import * as LabelPrimitive from '@radix-ui/react-label';
import type * as React from 'react';
import { cn } from '../../lib/utils';

function Label({
  className,
  ...props
}: React.ComponentProps<typeof LabelPrimitive.Root>) {
  return (
    <LabelPrimitive.Root
      className={cn(
        'font-body text-sm font-medium leading-none text-text-secondary peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
        className,
      )}
      {...props}
    />
  );
}

export { Label };
