import { Monitor, Moon, Sun } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useTheme } from './ThemeProvider';
import type { ThemePreference } from './theme';

const LABELS: Record<ThemePreference, string> = {
  system: 'System theme',
  light: 'Light theme',
  dark: 'Dark theme',
};

export function ThemeToggle({
  className,
  collapsed = false,
}: {
  className?: string;
  collapsed?: boolean;
}) {
  const { preference, cyclePreference } = useTheme();

  const Icon =
    preference === 'dark' ? Moon : preference === 'light' ? Sun : Monitor;

  return (
    <button
      type="button"
      data-testid="theme-toggle"
      data-theme-preference={preference}
      onClick={cyclePreference}
      className={cn(
        'inline-flex items-center rounded-md font-body text-sm text-text-secondary transition-colors duration-fast hover:bg-paper-200 hover:text-text-primary',
        collapsed ? 'justify-center px-2 py-2.5' : 'gap-2 px-3 py-1.5',
        className,
      )}
      aria-label={`${LABELS[preference]}. Click to change.`}
      title={LABELS[preference]}
    >
      <Icon className="h-4 w-4 shrink-0" aria-hidden />
      {!collapsed && <span className="capitalize">{preference}</span>}
    </button>
  );
}
