import type { LucideIcon } from 'lucide-react';
import {
  BookOpen,
  Bot,
  FolderGit2,
  LayoutGrid,
  LogOut,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Puzzle,
  Users,
  Wrench,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';
import { NotificationBell } from '../features/notifications/NotificationBell';
import { ThemeToggle } from '../features/theme/ThemeToggle';
import { useOpenTicket } from '../features/tickets/useOpenTicket';
import { cn } from '../lib/utils';
import { BrandLogo } from './BrandLogo';
import { DesktopUpdateBanner } from './DesktopUpdateBanner';

const SIDEBAR_COLLAPSED_KEY = 'coppice.sidebar.collapsed';

type NavItem = {
  to: string;
  label: string;
  icon: LucideIcon;
  adminOnly?: boolean;
};

const NAV_ITEMS: NavItem[] = [
  { to: '/boards', label: 'Boards', icon: LayoutGrid },
  { to: '/agents', label: 'Agents', icon: Bot },
  { to: '/chat', label: 'Chat', icon: MessageSquare },
  { to: '/knowledge', label: 'Knowledge', icon: BookOpen },
  { to: '/settings/repositories', label: 'Repositories', icon: FolderGit2 },
  { to: '/settings/plugins', label: 'Plugins', icon: Puzzle, adminOnly: true },
  { to: '/settings/users', label: 'Users', icon: Users, adminOnly: true },
  { to: '/tools', label: 'Tools', icon: Wrench, adminOnly: true },
];

function readSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === '1';
  } catch {
    return false;
  }
}

function navLinkClass({
  isActive,
  collapsed,
}: {
  isActive: boolean;
  collapsed: boolean;
}) {
  return cn(
    'flex items-center rounded-md font-body text-sm transition-colors duration-fast',
    collapsed ? 'justify-center px-2 py-2.5' : 'gap-3 px-3 py-2',
    isActive
      ? 'bg-accent-muted text-accent'
      : 'text-text-secondary hover:bg-paper-200 hover:text-text-primary',
  );
}

function isChatRoute(pathname: string): boolean {
  return pathname === '/chat' || pathname.startsWith('/chat/');
}

function isBoardRoute(pathname: string): boolean {
  return /^\/boards\/[^/]+\/?$/.test(pathname);
}

export function AppShell() {
  const { user, logout, desktopMode } = useSession();
  const openTicket = useOpenTicket();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const chatLayout = isChatRoute(pathname);
  const boardLayout = isBoardRoute(pathname);

  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCollapsed);

  useEffect(() => {
    try {
      localStorage.setItem(
        SIDEBAR_COLLAPSED_KEY,
        sidebarCollapsed ? '1' : '0',
      );
    } catch {
      /* ignore quota / private mode */
    }
  }, [sidebarCollapsed]);

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((prev) => !prev);
  }, []);

  const visibleNavItems = NAV_ITEMS.filter((item) => {
    if (desktopMode && item.to === '/settings/users') {
      return false;
    }
    return !item.adminOnly || user?.role === 'admin';
  });

  return (
    <div
      data-testid="app-shell-root"
      className={cn(
        'coppice-grain flex bg-background',
        chatLayout ? 'h-svh overflow-hidden' : 'min-h-screen',
      )}
    >
      <aside
        data-testid="app-shell-sidebar"
        data-collapsed={sidebarCollapsed ? 'true' : 'false'}
        className={cn(
          'flex shrink-0 flex-col border-r border-border bg-surface transition-[width] duration-200 ease-out',
          sidebarCollapsed ? 'w-[4.25rem]' : 'w-56',
        )}
        aria-label="Application sidebar"
      >
        <div
          data-testid="app-shell-sidebar-brand"
          className={cn(
            'flex h-14 shrink-0 items-center border-b border-border',
            sidebarCollapsed ? 'justify-center px-2' : 'gap-3 px-4',
          )}
        >
          <BrandLogo size={32} className="h-8 w-8 shrink-0" />
          {!sidebarCollapsed && (
            <span className="font-display text-lg font-semibold tracking-tight text-text-primary">
              Coppice
            </span>
          )}
        </div>

        <nav
          className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2"
          aria-label="Main"
        >
          {visibleNavItems.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              aria-label={sidebarCollapsed ? label : undefined}
              title={sidebarCollapsed ? label : undefined}
              className={({ isActive }) =>
                navLinkClass({ isActive, collapsed: sidebarCollapsed })
              }
            >
              <Icon className="h-[1.125rem] w-[1.125rem] shrink-0" aria-hidden />
              {!sidebarCollapsed && <span>{label}</span>}
            </NavLink>
          ))}
        </nav>

        <div className="shrink-0 border-t border-border p-2">
          <button
            type="button"
            onClick={toggleSidebar}
            className={cn(
              'flex w-full items-center rounded-md font-body text-sm text-text-secondary transition-colors duration-fast hover:bg-paper-200 hover:text-text-primary',
              sidebarCollapsed ? 'justify-center px-2 py-2.5' : 'gap-3 px-3 py-2',
            )}
            aria-expanded={!sidebarCollapsed}
            aria-label={
              sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'
            }
          >
            {sidebarCollapsed ? (
              <PanelLeftOpen className="h-[1.125rem] w-[1.125rem] shrink-0" />
            ) : (
              <>
                <PanelLeftClose className="h-[1.125rem] w-[1.125rem] shrink-0" />
                <span>Collapse</span>
              </>
            )}
          </button>
        </div>
      </aside>

      <div
        className={cn(
          'flex min-h-0 min-w-0 flex-1 flex-col',
          chatLayout && 'overflow-hidden',
        )}
      >
        <DesktopUpdateBanner />
        <header
          data-testid="app-shell-topbar"
          className="flex h-14 shrink-0 items-center justify-end gap-2 border-b border-border bg-surface px-4 sm:gap-4 sm:px-6"
        >
          {!desktopMode && (
            <span className="mr-auto hidden truncate font-body text-sm text-text-secondary sm:inline">
              {user?.email}
            </span>
          )}
          {desktopMode && <span className="mr-auto" />}
          <ThemeToggle />
          {!desktopMode && (
            <button
              type="button"
              onClick={() => void logout()}
              className="inline-flex items-center gap-2 rounded-md border border-border px-3 py-1.5 font-body text-sm text-text-secondary transition-colors duration-fast hover:border-border-strong hover:text-text-primary"
            >
              <LogOut className="h-4 w-4 shrink-0" aria-hidden />
              Sign out
            </button>
          )}
          {user && (
            <NotificationBell
              userId={user.id}
              onOpenTicket={openTicket}
              onOpenPath={(path) => void navigate(path)}
            />
          )}
        </header>

        <main
          className={cn(
            chatLayout
              ? 'flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-4 sm:px-6 sm:py-5'
              : 'w-full px-8 py-8',
          )}
          data-testid="app-shell-main"
          data-layout={chatLayout ? 'chat' : boardLayout ? 'board' : 'default'}
        >
          <Outlet />
        </main>
      </div>
    </div>
  );
}
