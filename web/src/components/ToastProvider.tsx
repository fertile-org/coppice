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
}

interface ToastApi {
  success: (message: string) => void;
  error: (message: string) => void;
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
    const timer = window.setTimeout(() => onDismiss(toast.id), TOAST_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [onDismiss, toast.id]);

  return (
    <div
      role="status"
      className={cn(
        'pointer-events-auto animate-fade-in rounded-md border px-4 py-3 font-body text-sm shadow-md',
        toast.variant === 'success' &&
          'border-success-muted bg-success-muted text-success',
        toast.variant === 'error' &&
          'border-danger-muted bg-danger-muted text-danger',
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

  const push = useCallback((message: string, variant: ToastVariant) => {
    const id = crypto.randomUUID();
    setToasts((current) => [...current, { id, message, variant }]);
  }, []);

  const api = useMemo<ToastApi>(
    () => ({
      success: (message) => push(message, 'success'),
      error: (message) => push(message, 'error'),
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
