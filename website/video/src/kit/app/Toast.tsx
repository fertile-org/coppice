import { X } from "lucide-react";
import React from "react";

// Mirrors web/src/components/ToastProvider.tsx: the region (fixed right-4 top-4,
// max-w-sm) and one success ToastMessage (message, Dismiss button, progress bar that
// empties over TOAST_DURATION_MS = 3000 ms). `enter` (0 → 1) is the 200 ms fade-in;
// `remaining` (1 → 0) is the progress bar's width.
//
// Render inside <AppShell> (it positions against the window, above a drawer).
export const Toast: React.FC<{ message: string; enter?: number; remaining?: number }> = ({
  message,
  enter = 1,
  remaining = 1,
}) => (
  <div className="absolute top-4 right-4 z-[100] flex w-full max-w-sm flex-col gap-2">
    <div
      className="relative overflow-hidden rounded-md border border-success-muted bg-success-muted font-body text-sm text-success shadow-md"
      style={{ opacity: enter }}
    >
      <div className="flex items-start gap-2 px-4 py-3">
        <div className="min-w-0 flex-1 text-left">{message}</div>
        <div className="shrink-0 rounded-md border border-border p-1 text-text-secondary">
          <X className="size-3.5" aria-hidden="true" />
        </div>
      </div>
      <div
        className="absolute bottom-0 left-0 h-0.5 bg-current"
        style={{ width: `${remaining * 100}%`, opacity: 0.4 }}
      />
    </div>
  </div>
);
