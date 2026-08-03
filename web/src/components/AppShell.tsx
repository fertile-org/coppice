import { NavLink, Outlet } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';
import { NotificationBell } from '../features/notifications/NotificationBell';
import { useOpenTicket } from '../features/tickets/useOpenTicket';

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  [
    'rounded-md px-3 py-1.5 font-body text-sm transition-colors duration-fast',
    isActive
      ? 'bg-accent-muted text-accent'
      : 'text-text-secondary hover:bg-paper-200 hover:text-text-primary',
  ].join(' ');

export function AppShell() {
  const { user, logout } = useSession();
  const openTicket = useOpenTicket();

  return (
    <div className="coppice-grain min-h-screen bg-background">
      <header className="border-b border-border bg-surface px-8 py-4">
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-6">
          <div className="flex items-center gap-6">
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

            <nav className="flex items-center gap-1" aria-label="Main">
              <NavLink to="/projects" className={navLinkClass}>
                Projects
              </NavLink>
              <NavLink to="/agents" className={navLinkClass}>
                Agents
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

          <div className="flex items-center gap-4">
            {user && (
              <NotificationBell userId={user.id} onOpenTicket={openTicket} />
            )}
            <span className="font-body text-sm text-text-secondary">
              {user?.email}
            </span>
            <button
              type="button"
              onClick={() => void logout()}
              className="rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
            >
              Sign out
            </button>
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-8 py-8">
        <Outlet />
      </main>
    </div>
  );
}
