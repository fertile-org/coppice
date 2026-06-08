import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { Loader2 } from 'lucide-react';
import type * as React from 'react';
import { cn } from '../../lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 rounded-md font-body text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default:
          'bg-accent px-4 py-2 text-white hover:bg-accent-hover',
        secondary:
          'border border-border bg-surface-raised px-3 py-1.5 text-text-secondary hover:text-text-primary',
        destructive:
          'border border-danger px-3 py-1.5 text-danger hover:bg-danger-muted/40',
        ghost: 'px-3 py-1.5 text-text-secondary hover:text-text-primary',
      },
      size: {
        default: '',
        sm: 'px-2.5 py-1 text-xs',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  loading = false,
  disabled,
  children,
  ...props
}: React.ComponentProps<'button'> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
    loading?: boolean;
  }) {
  const Comp = asChild ? Slot : 'button';

  if (asChild) {
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        {...props}
      >
        {children}
      </Comp>
    );
  }

  return (
    <Comp
      className={cn(buttonVariants({ variant, size, className }))}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {loading && (
        <Loader2 className="size-4 shrink-0 animate-spin" aria-hidden="true" />
      )}
      {children}
    </Comp>
  );
}

export { Button, buttonVariants };
