import type { ReactNode } from 'react';
import { sessionTheme } from '../theme/session-theme';

export function SessionSectionCard({ children }: { children: ReactNode }) {
  return (
    <div
      className={`border border-[var(--oc-border)] ${sessionTheme.bgPanel} px-3 py-2.5`}
    >
      {children}
    </div>
  );
}
