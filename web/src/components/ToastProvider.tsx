import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { cn } from '../lib/utils';

type ToastVariant = 'success' | 'error';

interface ToastItem {
  id: string;
  message: string;
  variant: ToastVariant;
  persistent?: boolean;
  onClick?: () => void;
}

interface ToastErrorOptions {
  persistent?: boolean;
  onClick?: () => void;
}

interface ToastApi {
  success: (message: string) => void;
  error: (message: string, opts?: ToastErrorOptions) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

const TOAST_DURATION_MS = 3000;

function ToastViewport({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: string) => void;
}) {
  if (toasts.length === 0) return null;

  return (
    <div
      aria-live="polite"
      aria-relevant="additions"
      className="pointer-events-none fixed right-4 top-4 z-[100] flex w-full max-w-sm flex-col gap-2"
    >
      {toasts.map((toast) => (
        <ToastMessage key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

function ToastMessage({
  toast,
  onDismiss,
}: {
  toast: ToastItem;
  onDismiss: (id: string) => void;
}) {
  useEffect(() => {
    if (toast.persistent) return;
    const timer = window.setTimeout(() => onDismiss(toast.id), TOAST_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [onDismiss, toast.id, toast.persistent]);

  function handleClick() {
    toast.onClick?.();
    onDismiss(toast.id);
  }

  const isClickable = Boolean(toast.onClick);

  return (
    <div
      role={isClickable ? 'button' : 'status'}
      tabIndex={isClickable ? 0 : undefined}
      onClick={isClickable ? handleClick : undefined}
      onKeyDown={
        isClickable
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleClick();
              }
            }
          : undefined
      }
      className={cn(
        'pointer-events-auto animate-fade-in rounded-md border px-4 py-3 font-body text-sm shadow-md',
        toast.variant === 'success' &&
          'border-success-muted bg-success-muted text-success',
        toast.variant === 'error' &&
          'border-danger-muted bg-danger-muted text-danger',
        isClickable && 'cursor-pointer hover:opacity-90',
      )}
    >
      {toast.message}
    </div>
  );
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const push = useCallback(
    (
      message: string,
      variant: ToastVariant,
      opts?: ToastErrorOptions,
    ) => {
      const id = crypto.randomUUID();
      setToasts((current) => [
        ...current,
        {
          id,
          message,
          variant,
          persistent: opts?.persistent,
          onClick: opts?.onClick,
        },
      ]);
    },
    [],
  );

  const api = useMemo<ToastApi>(
    () => ({
      success: (message) => push(message, 'success'),
      error: (message, opts) => push(message, 'error', opts),
    }),
    [push],
  );

  return (
    <ToastContext.Provider value={api}>
      {children}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToast must be used within ToastProvider');
  }
  return ctx;
}
