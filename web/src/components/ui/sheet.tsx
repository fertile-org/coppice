import * as DialogPrimitive from '@radix-ui/react-dialog';
import type * as React from 'react';
import { cn } from '../../lib/utils';
import { ignoreToastInteractions } from './overlay-interactions';
import {
  Dialog,
  DialogClose,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
} from './dialog';

const sheetSideClassName = {
  right:
    'inset-y-0 right-0 motion-safe:data-[state=open]:animate-sheet-in-right motion-safe:data-[state=closed]:animate-sheet-out-right',
} as const;

interface SheetContentProps
  extends React.ComponentProps<typeof DialogPrimitive.Content> {
  side?: keyof typeof sheetSideClassName;
  overlayClassName?: string;
  /** Other scrim props, e.g. `data-testid`. */
  overlayProps?: Omit<React.ComponentProps<typeof DialogPrimitive.Overlay>, 'className'> & {
    'data-testid'?: string;
  };
}

/**
 * Full-height drawer. Focuses the panel itself on open (not its first control)
 * unless `onOpenAutoFocus` prevents default.
 */
function SheetContent({
  className,
  side = 'right',
  overlayClassName,
  overlayProps,
  onOpenAutoFocus,
  onInteractOutside,
  ...props
}: SheetContentProps) {
  return (
    <DialogPortal>
      <DialogOverlay
        className={cn('backdrop-blur-[1px]', overlayClassName)}
        {...overlayProps}
      />
      <DialogPrimitive.Content
        className={cn(
          'fixed z-50 flex h-full w-full flex-col bg-surface-raised shadow-2xl focus:outline-none',
          sheetSideClassName[side],
          className,
        )}
        onOpenAutoFocus={(event) => {
          onOpenAutoFocus?.(event);
          if (event.defaultPrevented) return;
          event.preventDefault();
          (event.currentTarget as HTMLElement | null)?.focus();
        }}
        onInteractOutside={ignoreToastInteractions(onInteractOutside)}
        {...props}
      />
    </DialogPortal>
  );
}

export {
  Dialog as Sheet,
  DialogClose as SheetClose,
  SheetContent,
  DialogDescription as SheetDescription,
  DialogTitle as SheetTitle,
  DialogTrigger as SheetTrigger,
};
export type { SheetContentProps };
