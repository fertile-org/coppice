import { NavLink, Outlet } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  [
    'rounded-md px-3 py-1.5 font-body text-sm transition-colors duration-fast',
    isActive
      ? 'bg-accent-muted text-accent'
      : 'text-text-secondary hover:bg-paper-200 hover:text-text-primary',
  ].join(' ');

export function AppShell() {
  const { user, logout } = useSession();

  return (
    <div className="coppice-grain min-h-screen bg-background">
      <header className="border-b border-border bg-surface px-8 py-4">
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-6">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-3">
              <span
                className="inline-block h-3 w-3 rounded-full bg-accent"
                aria-hidden="true"
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
              {user?.role === 'admin' && (
                <NavLink to="/settings/users" className={navLinkClass}>
                  Users
                </NavLink>
              )}
            </nav>
          </div>

          <div className="flex items-center gap-4">
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
