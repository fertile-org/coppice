import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { Loader2 } from 'lucide-react';
import type * as React from 'react';
import { cn } from '../../lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 rounded-md font-body text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted disabled:cursor-not-allowed',
  {
    variants: {
      variant: {
        default:
          'bg-btn-primary px-4 py-2 text-btn-primary-fg hover:bg-btn-primary-hover',
        secondary:
          'border border-btn-secondary-border bg-btn-secondary px-3 py-1.5 text-btn-secondary-fg hover:bg-btn-secondary-hover hover:text-text-primary',
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
  // Loading keeps the enabled fill. A hollow disabled style would hide the spinner.
  const showDisabled = !asChild && Boolean(disabled) && !loading;
  const classes = cn(
    buttonVariants({ variant, size }),
    showDisabled &&
      'cursor-not-allowed border border-dashed border-btn-disabled-border bg-transparent text-btn-disabled-fg hover:border-btn-disabled-border hover:bg-transparent hover:text-btn-disabled-fg',
    className,
  );

  if (asChild) {
    return (
      <Comp className={classes} {...props}>
        {children}
      </Comp>
    );
  }

  return (
    <Comp
      className={classes}
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
