import type * as DialogPrimitive from '@radix-ui/react-dialog';
import type * as React from 'react';

type InteractOutsideHandler = NonNullable<
  React.ComponentProps<typeof DialogPrimitive.Content>['onInteractOutside']
>;

/**
 * `onInteractOutside` for modal content: toasts render outside the dialog, and
 * dismissing one must not also dismiss the dialog underneath.
 */
export function ignoreToastInteractions(
  handler?: InteractOutsideHandler,
): InteractOutsideHandler {
  return (event) => {
    handler?.(event);
    const target = event.target;
    if (target instanceof Element && target.closest('[data-toast-region]')) {
      event.preventDefault();
    }
  };
}
