import type { LucideIcon } from 'lucide-react';
import {
  BookOpen,
  Bot,
  ChevronDown,
  ChevronRight,
  DatabaseBackup,
  FolderGit2,
  LayoutGrid,
  LogOut,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Plug,
  RefreshCw,
  Puzzle,
  ScrollText,
  Settings,
  Users,
  Wrench,
} from 'lucide-react';
import { Fragment, useCallback, useEffect, useState } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { useSession } from '../features/auth/useSession';
import { NotificationBell } from '../features/notifications/NotificationBell';
import { ThemeToggle } from '../features/theme/ThemeToggle';
import { useOpenTicket } from '../features/tickets/useOpenTicket';
import { isDesktopShell } from '../lib/desktop';
import { cn } from '../lib/utils';
import { LOGS_COPY } from '../features/logs/copy';
import { SYSTEM_LABEL, SYSTEM_PATHS } from '../features/system/paths';
import { SkipToBoard } from '../features/keyboard/SkipToBoard';
import { BrandLogo } from './BrandLogo';
import { DesktopUpdateBanner } from './DesktopUpdateBanner';

const SIDEBAR_COLLAPSED_KEY = 'coppice.sidebar.collapsed';

type NavItem = {
  to: string;
  label: string;
  icon: LucideIcon;
  adminOnly?: boolean;
  /** Only inside the desktop app, which provides it. */
  desktopOnly?: boolean;
  end?: boolean;
};

const NAV_ITEMS: NavItem[] = [
  { to: '/boards', label: 'Boards', icon: LayoutGrid },
  { to: '/agents', label: 'Agents', icon: Bot },
  { to: '/chat', label: 'Chat', icon: MessageSquare },
  { to: '/knowledge', label: 'Knowledge', icon: BookOpen },
  { to: '/settings/repositories', label: 'Repositories', icon: FolderGit2 },
  { to: '/settings/plugins', label: 'Plugins', icon: Puzzle, adminOnly: true },
  { to: '/settings/users', label: 'Users', icon: Users, adminOnly: true },
  { to: '/settings', label: 'Settings', icon: Settings, adminOnly: true, end: true },
];

/** Admin-only group shown above Settings. */
const SYSTEM_ITEMS: NavItem[] = [
  { to: SYSTEM_PATHS.backup, label: 'Backup', icon: DatabaseBackup, adminOnly: true },
  { to: SYSTEM_PATHS.connectors, label: 'Connectors', icon: Plug, adminOnly: true },
  { to: SYSTEM_PATHS.logs, label: LOGS_COPY.nav, icon: ScrollText, adminOnly: true },
  { to: SYSTEM_PATHS.updates, label: 'Updates', icon: RefreshCw, adminOnly: true, desktopOnly: true },
];

function isSystemRoute(pathname: string): boolean {
  return pathname === '/system' || pathname.startsWith('/system/');
}

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

function SidebarLink({ item, collapsed }: { item: NavItem; collapsed: boolean }) {
  const { to, label, icon: Icon, end } = item;
  return (
    <NavLink
      to={to}
      end={end}
      aria-label={collapsed ? label : undefined}
      title={collapsed ? label : undefined}
      className={({ isActive }) => navLinkClass({ isActive, collapsed })}
    >
      <Icon className="h-[1.125rem] w-[1.125rem] shrink-0" aria-hidden />
      {!collapsed && <span>{label}</span>}
    </NavLink>
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
  const onSystemRoute = isSystemRoute(pathname);
  const [systemOpen, setSystemOpen] = useState(onSystemRoute);
  useEffect(() => {
    if (onSystemRoute) setSystemOpen(true);
  }, [onSystemRoute]);

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

  const showSystem = user?.role === 'admin';
  const systemItems = SYSTEM_ITEMS.filter((item) => !item.desktopOnly || isDesktopShell());
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
      {boardLayout && <SkipToBoard />}
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
          {visibleNavItems.map((item) => (
            <Fragment key={item.to}>
              {item.to === '/settings' && showSystem ? (
                sidebarCollapsed ? (
                  systemItems.map((child) => (
                    <SidebarLink key={child.to} item={child} collapsed />
                  ))
                ) : (
                  <div data-testid="nav-group-system">
                    <button
                      type="button"
                      onClick={() => setSystemOpen((open) => !open)}
                      aria-expanded={systemOpen}
                      aria-controls="nav-group-system-items"
                      className={cn(
                        navLinkClass({ isActive: false, collapsed: false }),
                        'w-full',
                        onSystemRoute && !systemOpen && 'text-accent',
                      )}
                    >
                      <Wrench className="h-[1.125rem] w-[1.125rem] shrink-0" aria-hidden />
                      <span className="flex-1 text-left">{SYSTEM_LABEL}</span>
                      {systemOpen ? (
                        <ChevronDown className="h-4 w-4 shrink-0" aria-hidden />
                      ) : (
                        <ChevronRight className="h-4 w-4 shrink-0" aria-hidden />
                      )}
                    </button>
                    {systemOpen ? (
                      <div
                        id="nav-group-system-items"
                        className="mt-0.5 flex flex-col gap-0.5 pl-4"
                      >
                        {systemItems.map((child) => (
                          <SidebarLink key={child.to} item={child} collapsed={false} />
                        ))}
                      </div>
                    ) : null}
                  </div>
                )
              ) : null}
              <SidebarLink item={item} collapsed={sidebarCollapsed} />
            </Fragment>
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
