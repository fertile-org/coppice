import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { X } from 'lucide-react';
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

  function handleAction() {
    toast.onClick?.();
    onDismiss(toast.id);
  }

  const isClickable = Boolean(toast.onClick);

  return (
    <div
      role="status"
      className={cn(
        'pointer-events-auto relative overflow-hidden rounded-md border font-body text-sm shadow-md animate-fade-in',
        toast.variant === 'success' &&
          'border-success-muted bg-success-muted text-success',
        toast.variant === 'error' &&
          'border-danger-muted bg-danger-muted text-danger',
      )}
    >
      <div className="flex items-start gap-2 px-4 py-3">
        {isClickable ? (
          <button
            type="button"
            onClick={handleAction}
            className="min-w-0 flex-1 cursor-pointer text-left hover:opacity-90"
          >
            {toast.message}
          </button>
        ) : (
          <p className="min-w-0 flex-1">{toast.message}</p>
        )}
        <button
          type="button"
          aria-label="Dismiss"
          onClick={(e) => {
            e.stopPropagation();
            onDismiss(toast.id);
          }}
          className="shrink-0 rounded-md border border-border p-1 text-text-secondary transition-colors duration-fast hover:text-text-primary"
        >
          <X className="size-3.5" aria-hidden="true" />
        </button>
      </div>
      {!toast.persistent && (
        <div
          data-testid="toast-progress"
          aria-hidden="true"
          className="absolute bottom-0 left-0 h-0.5 w-full bg-current opacity-40 animate-toast-progress"
          style={{ animationDuration: `${TOAST_DURATION_MS}ms` }}
        />
      )}
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
