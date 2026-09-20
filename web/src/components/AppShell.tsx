import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';
import { NotificationBell } from '../features/notifications/NotificationBell';
import { useOpenTicket } from '../features/tickets/useOpenTicket';
import { cn } from '../lib/utils';

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  [
    'rounded-md px-3 py-1.5 font-body text-sm transition-colors duration-fast',
    isActive
      ? 'bg-accent-muted text-accent'
      : 'text-text-secondary hover:bg-paper-200 hover:text-text-primary',
  ].join(' ');

function isChatRoute(pathname: string): boolean {
  return pathname === '/chat' || pathname.startsWith('/chat/');
}

export function AppShell() {
  const { user, logout } = useSession();
  const openTicket = useOpenTicket();
  const { pathname } = useLocation();
  const chatLayout = isChatRoute(pathname);

  return (
    <div
      className={cn(
        'coppice-grain bg-background',
        chatLayout
          ? 'flex h-svh flex-col overflow-hidden'
          : 'min-h-screen',
      )}
    >
      <header className="shrink-0 border-b border-border bg-surface px-4 py-3 sm:px-8 sm:py-4">
        <div
          className={cn(
            'mx-auto flex flex-wrap items-center justify-between gap-3 sm:gap-6',
            chatLayout ? 'max-w-none' : 'max-w-6xl',
          )}
        >
          <div className="flex w-full flex-wrap items-center gap-3 sm:w-auto sm:gap-6">
            <div className="flex items-center gap-3">
              <img
                src="/logo.webp"
                srcSet="/logo.webp 1x, /logo@2x.webp 2x"
                alt="Coppice"
                width={32}
                height={32}
                className="h-8 w-8 shrink-0"
              />
              <span className="font-display text-xl font-semibold tracking-tight text-text-primary">
                Coppice
              </span>
            </div>

            <nav className="flex flex-wrap items-center gap-1" aria-label="Main">
              <NavLink to="/projects" className={navLinkClass}>
                Projects
              </NavLink>
              <NavLink to="/agents" className={navLinkClass}>
                Agents
              </NavLink>
              <NavLink to="/chat" className={navLinkClass}>
                Chat
              </NavLink>
              <NavLink to="/knowledge" className={navLinkClass}>
                Knowledge
              </NavLink>
              <NavLink to="/settings/repositories" className={navLinkClass}>
                Repositories
              </NavLink>
              {user?.role === 'admin' && (
                <NavLink to="/settings/users" className={navLinkClass}>
                  Users
                </NavLink>
              )}
            </nav>
          </div>

          <div className="ml-auto flex items-center gap-2 sm:gap-4">
            <span className="hidden font-body text-sm text-text-secondary lg:inline">
              {user?.email}
            </span>
            <button
              type="button"
              onClick={() => void logout()}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
            >
              Sign out
            </button>
            {user && <NotificationBell userId={user.id} onOpenTicket={openTicket} />}
          </div>
        </div>
      </header>

      <main
        className={cn(
          chatLayout
            ? 'flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-4 sm:px-6 sm:py-5'
            : 'mx-auto max-w-6xl px-8 py-8',
        )}
        data-testid="app-shell-main"
        data-layout={chatLayout ? 'chat' : 'default'}
      >
        <Outlet />
      </main>
    </div>
  );
}
