import type { ReactNode } from 'react';
import { sessionTheme } from '../theme/session-theme';

export function OutputBlock({
  children,
  className = '',
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`${sessionTheme.outputBlock} ${sessionTheme.fontMono} ${sessionTheme.text} ${className}`}>
      {children}
    </div>
  );
}
