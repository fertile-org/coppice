import { cva, type VariantProps } from 'class-variance-authority';
import type * as React from 'react';
import { cn } from '../../lib/utils';

const badgeVariants = cva(
  'inline-flex items-center rounded-full border px-2.5 py-0.5 font-body text-xs font-medium transition-colors',
  {
    variants: {
      variant: {
        default: 'border-border bg-surface text-text-secondary',
        outline: 'border-border bg-transparent text-text-secondary',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { Badge, badgeVariants };
